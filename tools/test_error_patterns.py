"""Lint explicit Rust error declarations (Python 3.11+, standard library only).

Run `python3 tools/test_error_patterns.py [files-or-directories ...]` to audit
production sources, or `--self-test` to test the checker. No baseline or automatic
fixes. Existing violations fail the audit and CI.

Syntactic checks cover naming, thiserror derives, handwritten error traits,
string/unit Result errors, erased error types, catch-all error variants, generic
message payloads and string-to-error conversions. Domain text fields are allowed.
Dependency-only messages require an adjacent `// ERROR-DEPENDENCY: dep::api`
comment on a named `message: String` field. This records provenance, not an allow
for other rules; whether the API actually returns a string still needs review.

Uses the repository's Rust item scanner. Comments, literals, test files and
cfg(test) items are excluded. Resolves simple non-generic type alias chains in
their declaring module and recognizes grouped/renamed anyhow and eyre imports.
Does not resolve cross-module imports, generic/associated aliases, aliases inside
function bodies, or macro expansions, or prove ownership and payload semantics.
Compiler checks and code review remain necessary. Operation errors are identified
by an Error derive/trait implementation or a directly named Result error argument in the same file, never by a name suffix
alone. Cross-file error uses and payload roles still need review.
"""
import argparse
from pathlib import Path
import re
import sys
import unittest

from test_module_layout import SourceFile, matching

ROOT = Path(__file__).resolve().parents[1]
TEXT = r"(?:String|(?:std|alloc)::string::String|&\s*(?:'\w+\s+)?str)"


def live_items(items):
    """Yield production items, excluding entire test-only subtrees."""
    for item in items:
        if not item.test_only:
            yield item
            yield from live_items(item.children)


def strip_attributes(code):
    """Mask complete attributes while retaining offsets, including nested tokens."""
    result = list(code)
    for match in re.finditer(r'#\s*\[', code):
        end = matching(code, match.end() - 1, len(code)) + 1
        result[match.start():end] = ['\n' if char == '\n' else ' '
                                    for char in code[match.start():end]]
    return ''.join(result)


def result_errors(code):
    """Yield direct Result error arguments with nested tuple/array/generic support."""
    for match in re.finditer(r"\bResult\s*<", code):
        stack, comma = ['<'], None
        for index in range(match.end(), len(code)):
            char = code[index]
            if char in '<([{':
                stack.append(char)
            elif char in '>)]}' and not (char == '>' and code[index - 1] == '-'):
                if not stack:
                    break
                stack.pop()
                if not stack:
                    if comma is not None:
                        yield match.start(), code[comma + 1:index].strip().rstrip(',').strip()
                    break
            elif char == ',' and stack == ['<'] and comma is None:
                comma = index


def import_paths(tree, prefix=''):
    """Expand a Rust use tree into imported paths, ignoring local rename spellings."""
    index, start = 0, 0
    while index <= len(tree):
        if index < len(tree) and tree[index] == '{':
            index = matching(tree, index, len(tree)) + 1
            continue
        if index == len(tree) or tree[index] == ',':
            branch = tree[start:index].strip()
            opening = branch.find('{')
            if opening >= 0:
                closing = matching(branch, opening, len(branch))
                yield from import_paths(branch[opening + 1:closing],
                                        prefix + branch[:opening].strip())
            elif branch:
                path = re.split(r'\s+as\s+', branch)[0].strip()
                yield re.sub(r'\s+', '', prefix + path).lstrip(':')
            start = index + 1
        index += 1


def alias_resolver(code, items):
    """Resolve non-generic aliases within their declaring module, with cycle guards."""
    scopes = []

    def collect(children, start, end):
        aliases = {}
        for item in children:
            if item.test_only:
                continue
            if item.kind == 'type':
                fragment = strip_attributes(code[item.start:item.end])
                match = re.search(r'\btype\s+(\w+)\s*=\s*(.*?)\s*;', fragment, re.S)
                if match:
                    aliases[match[1]] = match[2]
            elif item.kind == 'mod' and item.body:
                collect(item.children, *item.body)
        scopes.append((start, end, aliases))

    collect(items, 0, len(code))

    def resolve(name, offset):
        # A child module has its own namespace, even if it defines no aliases.
        _, _, aliases = min((scope for scope in scopes if scope[0] <= offset < scope[1]),
                            key=lambda scope: scope[1] - scope[0])
        seen = set()
        while name in aliases and name not in seen:
            seen.add(name)
            name = aliases[name].strip()
        return name

    return resolve


def derives_error(fragment):
    """Recognize qualified and conventionally imported thiserror derives."""
    header = fragment[:re.search(r'\b(?:struct|enum)\b', fragment).start()]
    derives = re.findall(r'#\s*\[\s*derive\s*\((.*?)\)\s*\]', header, re.S)
    return any(re.search(r'(?:^|,)\s*(?:thiserror::)?Error\s*(?:,|$)', derive)
               for derive in derives)


def violations(path, source):
    """Return deterministic (line, rule, detail) findings for one source file."""
    parsed = SourceFile.parse(path, source)
    items = list(live_items(parsed.items))
    # Remove test bodies while preserving source coordinates.
    code = list(parsed.code)

    def mask_tests(children):
        for item in children:
            if item.test_only:
                code[item.start:item.end] = ['\n' if c == '\n' else ' '
                                             for c in code[item.start:item.end]]
            else:
                mask_tests(item.children)

    mask_tests(parsed.items)
    code = ''.join(code)
    findings = set()

    def report(offset, rule, detail):
        findings.add((parsed.line(offset) + 1, rule, detail))

    resolve_alias = alias_resolver(code, parsed.items)
    outside_imports = list(code)
    for item in items:
        if item.kind == 'use':
            outside_imports[item.start:item.end] = ['\n' if char == '\n' else ' '
                                                   for char in code[item.start:item.end]]
            fragment = strip_attributes(code[item.start:item.end])
            tree = re.search(r'\buse\s+(.*);', fragment, re.S)
            if tree:
                for imported in import_paths(tree[1]):
                    if imported.split('::')[0] in {'anyhow', 'eyre'}:
                        report(item.start, 'erased-error', imported)

    errors = {item.name for item in items if item.kind in {'struct', 'enum'}
              and derives_error(code[item.start:item.end])}
    for offset, error in result_errors(code):
        error = resolve_alias(error, offset)
        if re.fullmatch(TEXT + r"|\(\)", error):
            report(offset, 'untyped-result', error)
        named = re.fullmatch(r"(?:\w+::)*(\w+)(?:\s*<.*>)?", error, re.S)
        if named:
            errors.add(named[1])
    for item in items:
        if item.kind == 'impl' and item.impl_trait and item.impl_trait.split('::')[-1] == 'Error':
            errors.add(item.impl_type)
    for item in items:
        fragment = code[item.start:item.end]
        if item.kind == 'impl' and item.impl_type in errors:
            trait = (item.impl_trait or '').split('::')[-1]
            if trait in {'Display', 'Error'}:
                report(item.start, 'manual-error-trait', item.impl_type)
            if re.search(r'\bFrom\s*<\s*' + TEXT + r'\s*>', fragment):
                report(item.start, 'string-conversion', item.impl_type)
        if item.kind not in {'struct', 'enum'} or item.name not in errors:
            continue
        if not item.name.endswith('Error') or item.name == 'Error':
            report(item.start, 'error-name', item.name)
        if not derives_error(fragment):
            report(item.start, 'missing-thiserror', item.name)
        plain = strip_attributes(fragment)
        if item.kind == 'enum' and item.body:
            body = strip_attributes(code[item.body[0]:item.body[1]])
            for match in re.finditer(r'(?:^|[,}])\s*(?:#\[[^\]]*\]\s*)*'
                                     r'(Unknown|Other|Custom|Internal|Message)\b', body):
                report(item.body[0] + match.start(), 'catch-all-error', match[1])
        if re.search(r'\(\s*(?:pub(?:\([^)]*\))?\s+)?' + TEXT + r'\s*[,)]', plain):
            report(item.start, 'string-error-payload', item.name)
        for match in re.finditer(r'\b(detail|msg|message|reason|description|diagnostic|error|context)\s*:\s*' + TEXT + r'(?=\s*[,}])', plain):
            offset = item.start + match.start()
            line = parsed.line(offset)
            marker = parsed.lines[line - 1].strip() if line else ''
            inherited = item.kind == 'enum' and match[1] == 'message' and re.fullmatch(
                r'// ERROR-DEPENDENCY: [a-zA-Z_]\w*(?:::[a-zA-Z_]\w*)+', marker)
            if not inherited:
                report(offset, 'message-error-payload', item.name + '.' + match[1])
    for match in re.finditer(r'\bErr\s*\(', code):
        original = source[match.end():].lstrip()
        if re.match(r'(?:"|r#*"|format!\s*\()', original):
            report(match.start(), 'authored-error-text', 'Err contains a diagnostic string')
    for match in re.finditer(r'(?<![\w:])(?:::)?(?:anyhow|eyre)::(?:Error|Report|Result)\b|'
                             r'\bBox\s*<\s*dyn\s+(?:[\w]+::)*Error\b', ''.join(outside_imports)):
        report(match.start(), 'erased-error', match[0])
    return sorted(findings)


class ErrorPatternTests(unittest.TestCase):
    """Check rules against valid and invalid Rust syntax fixtures."""

    def rules(self, source):
        return {rule for _, rule, _ in violations('src/example.rs', source)}

    def test_typed_context_and_causes(self):
        source = '''#[derive(Debug, thiserror::Error)]
        enum ReadError {
            #[error("read failed")]
            Read { path: PathBuf, #[source] source: std::io::Error },
            #[error("invalid name {name}")]
            InvalidName { name: String },
        }'''
        self.assertEqual(self.rules(source), set())

    def test_naming_derive_and_manual_traits(self):
        self.assertEqual(self.rules('enum ReadFailure {}\nimpl std::error::Error for ReadFailure {}'),
                         {'error-name', 'missing-thiserror', 'manual-error-trait'})
        self.assertIn('error-name', self.rules('#[derive(Error)] enum Error {}'))
        self.assertIn('error-name', self.rules('struct Oops; fn f() -> Result<(), Oops> {}'))
        self.assertIn('error-name', self.rules('enum Failure<E> {} fn f() -> Result<(), Failure<E>> {}'))
        self.assertIn('manual-error-trait', self.rules('struct ReadError; impl fmt::Display for ReadError {} fn f() -> Result<(), ReadError> {}'))

    def test_string_carriers_and_conversions(self):
        self.assertIn('untyped-result', self.rules("type R = Result<Vec<u8>, &'static str>;"))
        self.assertIn('untyped-result', self.rules('fn f() -> Result<Vec<Vec<u8>>, String> {}'))
        self.assertIn('untyped-result', self.rules('type R = Result<(), ()>;'))
        self.assertIn('string-error-payload', self.rules('#[derive(thiserror::Error)] struct ReadError(pub String);'))
        self.assertIn('string-conversion', self.rules('#[derive(thiserror::Error)] struct ReadError; impl From<String> for ReadError {}'))
        self.assertIn('message-error-payload', self.rules('#[derive(thiserror::Error)] enum ReadError { Bad { detail: String } }'))

    def test_dependency_message_requires_provenance(self):
        declaration = '#[derive(thiserror::Error)] enum DecodeError { Backend {\n%s message: String } }'
        self.assertEqual(self.rules(declaration % '// ERROR-DEPENDENCY: backend::decode\n'), set())
        self.assertIn('message-error-payload', self.rules(declaration % '// backend problem\n'))

    def test_catch_all_variants_and_erasure(self):
        self.assertIn('catch-all-error', self.rules('#[derive(Error)] enum ReadError { Other(IoError) }'))
        for error in ['anyhow::Result<()>', 'eyre::Report', '::eyre::Report', 'Box<dyn std::error::Error + Send>']:
            self.assertIn('erased-error', self.rules('type R = ' + error + ';'))

    def test_ignores_comments_literals_and_tests(self):
        self.assertEqual(self.rules('''// enum BadError { Other(String) }
            const S: &str = r#"anyhow::Error"#;
            #[cfg(test)] mod tests { enum BadError { Other(String) } }
            fn f() { let text = "Result<(), String>"; }
        '''), set())

    def test_values_and_context_are_not_errors_by_name(self):
        self.assertEqual(self.rules('''
            enum CellError { Value } impl fmt::Display for CellError {}
            enum ExcelError { Value }
            struct CaseFailure { message: String }
            struct QuotaFailure { limit: usize }
        '''), set())

    def test_annotated_string_tuple_payload(self):
        self.assertIn('string-error-payload', self.rules('''
            #[derive(thiserror::Error)]
            struct ValueError(#[doc = "diagnostic"] pub String);
        '''))

    def test_nested_result_success_and_error_types(self):
        self.assertEqual(list(result_errors('Result<(u8, u8), String>')),
                         [(0, 'String')])
        self.assertEqual(list(result_errors('Result<fn(u8, u8) -> (), String>')),
                         [(0, 'String')])
        self.assertEqual(list(result_errors('Result<[u8; 2], ReadError,>')),
                         [(0, 'ReadError')])

    def test_authored_diagnostic_field_synonyms(self):
        for field in ['detail', 'msg', 'message', 'reason', 'description', 'diagnostic', 'error', 'context']:
            self.assertIn('message-error-payload', self.rules(
                '#[derive(thiserror::Error)] struct ReadError { ' + field + ': String }'))

    def test_dependency_marker_cannot_allow_other_strings(self):
        self.assertIn('message-error-payload', self.rules('''
            #[derive(thiserror::Error)] struct DecodeError {
                // ERROR-DEPENDENCY: backend::decode
                detail: String,
            }
        '''))
        self.assertIn('message-error-payload', self.rules('''
            #[derive(thiserror::Error)] struct DecodeError {
                // ERROR-DEPENDENCY: unspecified
                message: String,
            }
        '''))

    def test_dependency_text_requires_named_enum_variant(self):
        self.assertIn('message-error-payload', self.rules('''
            #[derive(thiserror::Error)] struct DecodeError {
                // ERROR-DEPENDENCY: backend::decode
                message: String,
            }
        '''))
        self.assertIn('catch-all-error', self.rules('''
            #[derive(thiserror::Error)] enum DecodeError { Other {
                // ERROR-DEPENDENCY: backend::decode
                message: String,
            } }
        '''))

    def test_inferred_authored_error_strings(self):
        for value in ['"bad input"', 'r#"bad input"#', 'format!("bad {}", value)']:
            self.assertIn('authored-error-text', self.rules('fn f() { Err(' + value + ') }'))
        self.assertNotIn('authored-error-text', self.rules('fn f() { Err(ReadError::Invalid) }'))

    def test_imported_error_derive(self):
        self.assertEqual(self.rules('use thiserror::Error; #[derive(Debug, Error)] enum ReadError {}'), set())

    def test_string_and_unit_alias_chains_in_error_position(self):
        for target in ['String', 'std::string::String', "&'static str", '()']:
            with self.subTest(target=target):
                self.assertIn('untyped-result', self.rules(
                    'type Diagnostic = ' + target + '; type Alias = Diagnostic; '
                    'fn read() -> Result<(), Alias> {}'))

    def test_text_domain_and_typed_error_aliases(self):
        self.assertEqual(self.rules('''
            type Identifier = String;
            type ReadAlias = ReadError;
            #[derive(thiserror::Error)] enum ReadError { Invalid { name: Identifier } }
            fn read() -> Result<Identifier, ReadAlias> {}
        '''), set())

    def test_module_alias_shadowing(self):
        self.assertEqual(self.rules('''
            type Diagnostic = String;
            mod inner {
                type Diagnostic = ReadError;
                #[derive(thiserror::Error)] enum ReadError { Invalid }
                fn read() -> Result<(), Diagnostic> {}
            }
        '''), set())

    def test_aliases_are_resolved_inside_inline_modules(self):
        self.assertIn('untyped-result', self.rules('''
            mod inner {
                type Diagnostic = String;
                fn read() -> Result<(), Diagnostic> {}
            }
        '''))

    def test_parent_aliases_do_not_leak_into_child_namespaces(self):
        self.assertEqual(self.rules('''
            type Diagnostic = String;
            mod inner {
                use dependency::Diagnostic;
                fn read() -> Result<(), Diagnostic> {}
            }
        '''), set())

    def test_alias_cycles_do_not_hang(self):
        self.assertEqual(self.rules(
            'type A = B; type B = A; fn read() -> Result<(), A> {}'), set())

    def test_erased_framework_imports(self):
        for statement in [
            'use anyhow::{Result, Error};',
            'use eyre::{Report as Diagnostic, Result as R};',
            'use anyhow::Error as Diagnostic;',
            'use anyhow as diagnostic;',
            'use {anyhow::{Error as Diagnostic}, std::path::Path};',
            'use ::eyre::{self, Report};',
        ]:
            with self.subTest(statement=statement):
                self.assertIn('erased-error', self.rules(statement))
        self.assertEqual(self.rules('use crate::domain::{ReadError as Diagnostic};'), set())
        self.assertEqual(self.rules('use domain::{anyhow::Error as Diagnostic};'), set())

    def test_lines_survive_masking(self):
        findings = violations('src/a.rs', '// ignored\n\nfn f() -> Result<(), String> {}')
        self.assertEqual(findings, [(3, 'untyped-result', 'String')])


def main():
    """Run checker fixtures or audit requested production paths."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--self-test', action='store_true')
    parser.add_argument('paths', nargs='*', type=Path)
    args = parser.parse_args()
    if args.self_test:
        return not unittest.TextTestRunner(verbosity=2).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(ErrorPatternTests)).wasSuccessful()
    paths = args.paths or [ROOT / 'src', *sorted((ROOT / 'crates').glob('*/src'))]
    files = set()
    for path in paths:
        if not path.exists():
            parser.error(f'path does not exist: {path}')
        files.update(path.rglob('*.rs') if path.is_dir() else [path])
    count = 0
    for path in sorted(files):
        if 'tests' in path.parts or path.name == 'tests.rs' or path.stem.endswith('_tests'):
            continue
        relative = path.resolve().relative_to(ROOT) if path.resolve().is_relative_to(ROOT) else path.resolve()
        for line, rule, detail in violations(relative.as_posix(), path.read_text()):
            print(f'{relative}:{line}: {rule}: {detail}')
            count += 1
    print(f'{count} error-pattern violation(s)')
    return bool(count)


if __name__ == '__main__':
    sys.exit(main())
