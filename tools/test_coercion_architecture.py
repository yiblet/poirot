"""Guard explicit spreadsheet coercion boundaries (Python 3.11+, no dependencies).

This is a syntactic guard, not a Rust type checker: it catches legacy entry-point
names and common inline type changes. Review still has to identify new policies.
Numeric algorithm casts and type-preserving storage/encoding are outside scope.
"""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


def mask_literals_and_comments(source):
    """Keep offsets/braces stable while removing Rust comments and literals."""
    output = list(source)
    token = re.compile(r'//[^\n]*|/\*|(?:br|r)(?P<hashes>\#*)"|b?"|b?\'(?:\\.|[^\'\\\n])\'')
    at = 0
    while match := token.search(source, at):
        start = match.start()
        head = match[0]
        end = match.end()
        if head == '/*':
            depth = 1
            while depth and end < len(source):
                pair = re.search(r'/\*|\*/', source[end:])
                if pair is None:
                    end = len(source)
                    break
                end += pair.end()
                depth += 1 if pair[0] == '/*' else -1
        elif match['hashes'] is not None:
            closing = '"' + match['hashes']
            close = source.find(closing, end)
            end = len(source) if close < 0 else close + len(closing)
        elif head in ('"', 'b"'):
            while end < len(source):
                char = source[end]
                end += 1
                if char == '\\':
                    end += 1
                elif char == '"':
                    break
        for i in range(start, min(end, len(source))):
            if source[i] != '\n':
                output[i] = ' '
        at = end
    return ''.join(output)


def close_brace(code, opening):
    depth = 1
    for index in range(opening + 1, len(code)):
        depth += (code[index] == '{') - (code[index] == '}')
        if depth == 0:
            return index
    return len(code)


def function_spans(code):
    for match in re.finditer(r'\bfn\s+(\w+)\b[^;{]*([;{])', code):
        if match[2] == '{':
            opening = match.end() - 1
            yield match[1], opening, close_brace(code, opening)


def production_code(source):
    code = mask_literals_and_comments(source)
    # Skip inline test-only items, including modules; separate test files are
    # excluded when enumerating source paths below.
    for match in reversed(list(re.finditer(r'#\[cfg\(test\)\]', code))):
        start = match.end()
        item = re.search(r'[;{]', code[start:])
        if item is None:
            continue
        opening = start + item.start()
        end = close_brace(code, opening) + 1 if item[0] == '{' else opening + 1
        code = code[:match.start()] + re.sub(r'[^\n]', ' ', code[match.start():end]) + code[end:]
    return code


# Match common scalar kind changes, not comparisons/classification or forwarding
# a typed blank. Deliberately do not flag every cast in numeric algorithms.
INLINE_CHANGE = re.compile(
    r'\b(?:\w+::)*(?:Blank|Missing)\b[^\n=]*=>\s*'
    r'(?:return\s+)?(?:'
    r'(?:Ok\(|Some\()\s*(?:Number\(|CellValue::Number\(|0(?:\.0?)?(?![\w.])|false\b|String::new\()'
    r'|(?:Number|Text|String::new)\()'
    r'|\b(?:\w+::)*(?:Boolean|Bool)\([^\n]*=>[^\n]*'
    r'(?:as f64|u8::from|Number\(|String::new|Text\(|1\.0)'
)



def violations(source):
    code = production_code(source)
    spans = list(function_spans(code))
    result = []
    patterns = [INLINE_CHANGE, re.compile(
        r'\bif\b[^{};]*\b(?:Blank|Missing)\b[^{};]*\{\s*'
        r'(?:return\s+)?(?:Ok\(|Some\()?(?:Number|Text|String::new)\('
    )]
    matches = [match for pattern in patterns for match in pattern.finditer(code)]
    for match in matches:
        containing = [(name, start) for name, start, end in spans if start <= match.start() < end]
        if not containing:
            continue
        name = max(containing, key=lambda item: item[1])[0]
        if not name.startswith(('coerce_', 'coercev2_')) and name != 'coercev2':
            result.append((code.count('\n', 0, match.start()) + 1, name, 'inline type change outside coerce_ helper'))
        elif name.startswith('coerce_') and re.search(r'\bBlank\b', match[0]) and re.search(r'\b(?:Number|0)\b', match[0]):
            result.append((code.count('\n', 0, match.start()) + 1, name, 'blank-to-zero conversion outside argument policy'))
    for match in re.finditer(r'\b(?:finalize_cell_result|operand_number|operand_boolean|logical_test|scalar_number|scalar_text)\s*\(', code):
        result.append((code.count('\n', 0, match.start()) + 1, '', 'legacy coercion helper name'))
    for match in re.finditer(r'\bcoerce_\w+\s+as\s+(?!coerce_)\w+', code):
        result.append((code.count('\n', 0, match.start()) + 1, '', 'coercion import hides its prefix'))
    return result


def searchable_v2_source(source):
    """Permit only locally declared, adjacent-audited non-argument names."""
    code = production_code(source)
    declared_results = set()
    for match in re.finditer(r'\bfn\s+(coerce_\w+)\b', code):
        line_start = source.rfind('\n', 0, match.start())
        prefix = source[:line_start].splitlines()
        adjacent = prefix[-1] if prefix else ''
        if re.search(r'// TODO\(coercion-non-argument\): \[(?:HIGH|MEDIUM|LOW)\] \S.+', adjacent):
            declared_results.add(match[1])
    source = re.sub(r'// TODO\(coercion-non-argument\):[^\n]*', '', source)
    for name in declared_results:
        source = re.sub(r'\b' + re.escape(name) + r'\b', '', source)
    return source


class CoercionArchitectureTests(unittest.TestCase):
    def test_type_changes_live_in_named_coercion_helpers(self):
        failures = []
        crates = [ROOT / 'src/eval', ROOT / 'crates/poirot-functions/src']
        for crate in crates:
            for path in sorted(crate.rglob('*.rs')):
                if any('test' in part for part in path.relative_to(crate).parts):
                    continue
                for line, name, reason in violations(path.read_text()):
                    failures.append(f'{path.relative_to(ROOT)}:{line}: {name}: {reason}')
        self.assertEqual(failures, [], '\n' + '\n'.join(failures))

    def test_remaining_legacy_conversion_declarations_are_audited(self):
        failures = []
        for path in [*(ROOT / 'crates/poirot-functions/src').rglob('*.rs'), *(ROOT / 'src/eval').rglob('*.rs')]:
            if 'test' in path.name:
                continue
            source = path.read_text()
            code = production_code(source)
            for match in re.finditer(r'\bfn\s+(coerce_\w+)\b', code):
                line_start = source.rfind('\n', 0, match.start())
                prefix = source[:line_start].splitlines()
                adjacent = prefix[-1] if prefix else ''
                if '// TODO(coercion-non-argument):' not in adjacent:
                    failures.append(f'{path.relative_to(ROOT)}: {match[1]} lacks adjacent audit marker')
        self.assertEqual(failures, [], '\n' + '\n'.join(failures))

    def test_migrated_argument_helpers_do_not_keep_inline_conversions(self):
        failures = []
        for crate in [ROOT / 'src/eval', ROOT / 'crates/poirot-functions/src']:
            for path in crate.rglob('*.rs'):
                if 'test' in path.name or 'coercion' in path.relative_to(crate).parts:
                    continue
                code = production_code(path.read_text())
                spans = list(function_spans(code))
                for match in INLINE_CHANGE.finditer(code):
                    containing = [(name, start) for name, start, end in spans
                                  if start <= match.start() < end]
                    if containing:
                        name = max(containing, key=lambda item: item[1])[0]
                        if name.startswith('coercev2_'):
                            failures.append(f'{path.relative_to(ROOT)}: {name} retains an inline argument conversion')
        self.assertEqual(failures, [], '\n' + '\n'.join(failures))

    def test_non_argument_todos_have_risk_ratings(self):
        todos = []
        for path in [*(ROOT / 'crates/poirot-functions/src').rglob('*.rs'), *(ROOT / 'src/eval').rglob('*.rs')]:
            for line in path.read_text().splitlines():
                if '// TODO(coercion-non-argument):' in line:
                    todos.append(line)
                    self.assertRegex(line, r'// TODO\(coercion-non-argument\): \[(?:HIGH|MEDIUM|LOW)\] \S.+')
        self.assertTrue(todos, 'non-argument conversion audit must remain visible')

    def test_guard_catches_inline_coercions_and_hidden_aliases(self):
        self.assertTrue(violations('fn select(v: V) { match v { Blank => Number(0.), _ => v } }'))
        self.assertTrue(violations('fn select(v: V) { match v { Boolean(b) => u8::from(b) as f64, _ => 0.0 } }'))
        self.assertTrue(violations('use foo::coerce_number as number;'))
        self.assertTrue(violations('fn select(v: V) { finalize_cell_result(v) }'))
        self.assertTrue(violations('fn select(v: V) { if v == Blank { Number(0.) } else { v } }'))

    def test_guard_preserves_typed_reads_algorithms_and_test_exemptions(self):
        self.assertFalse(violations('fn coercev2_input(v: V) { match v { Blank => Number(0.) } }'))
        self.assertTrue(violations('fn coerce_publication(v: V) { match v { Blank => Number(0.), _ => v } }'))
        self.assertFalse(violations('fn read(v: V) { match v { Blank => Blank, _ => v } }'))
        self.assertFalse(violations('fn kernel(n: usize) { n as f64 }'))
        self.assertFalse(violations('#[cfg(test)] mod tests { fn sample() { match v { Blank => Number(0.) } } }'))
        self.assertFalse(violations('fn read() { /* nested /* Blank => Number(0.) */ comment */ let s = r#"Blank => Number(0.)"#; }'))
        # A production item after a test-only item is still checked.
        self.assertTrue(violations('#[cfg(test)] mod tests {} fn read() { match v { Blank => Number(0.) } }'))

    def test_v2_naming_exempts_only_audited_central_result_helpers(self):
        pattern = r'(?i)\b(?:coercion|coerce)(?!v2)'
        audited = '// TODO(coercion-non-argument): [LOW] Result text may round digits.\nfn coerce_result() {}\nfn use_result() { coerce_result() }'
        self.assertNotRegex(searchable_v2_source(audited), pattern)
        self.assertRegex(searchable_v2_source('fn coerce_result() {}'), pattern)
        self.assertRegex(searchable_v2_source('fn use_result() { coerce_result() }'), pattern)
        self.assertRegex(searchable_v2_source(audited + '\nfn use_other() { coerce_argument() }'), pattern)

    def test_v2_crate_keeps_conversion_names_searchable(self):
        crate = ROOT / 'crates/poirot-functions/src/coercion'
        for path in crate.rglob('*.rs'):
            source = path.read_text()
            if path.suffix == '.rs':
                source = searchable_v2_source(source)
            self.assertNotRegex(production_code(source), r'\bcoerce_(?!v2)\w+', str(path))


if __name__ == '__main__':
    unittest.main()
