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
        if not name.startswith('coerce_'):
            result.append((code.count('\n', 0, match.start()) + 1, name, 'inline type change outside coerce_ helper'))
    for match in re.finditer(r'\b(?:finalize_cell_result|operand_number|operand_boolean|logical_test|scalar_number|scalar_text)\s*\(', code):
        result.append((code.count('\n', 0, match.start()) + 1, '', 'legacy coercion helper name'))
    for match in re.finditer(r'\bcoerce_\w+\s+as\s+(?!coerce_)\w+', code):
        result.append((code.count('\n', 0, match.start()) + 1, '', 'coercion import hides its prefix'))
    return result


class CoercionArchitectureTests(unittest.TestCase):
    def test_type_changes_live_in_named_coercion_helpers(self):
        failures = []
        crates = [ROOT / 'crates' / 'poirot-eval', *sorted((ROOT / 'crates').glob('poirot-functions-*'))]
        for crate in crates:
            for path in sorted((crate / 'src').rglob('*.rs')):
                if any('test' in part for part in path.relative_to(crate / 'src').parts):
                    continue
                for line, name, reason in violations(path.read_text()):
                    failures.append(f'{path.relative_to(ROOT)}:{line}: {name}: {reason}')
        self.assertEqual(failures, [], '\n' + '\n'.join(failures))

    def test_non_argument_todos_have_risk_ratings(self):
        todos = []
        for path in (ROOT / 'crates').rglob('*.rs'):
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
        self.assertFalse(violations('fn coerce_publication(v: V) { match v { Blank => Number(0.), _ => v } }'))
        self.assertFalse(violations('fn read(v: V) { match v { Blank => Blank, _ => v } }'))
        self.assertFalse(violations('fn kernel(n: usize) { n as f64 }'))
        self.assertFalse(violations('#[cfg(test)] mod tests { fn sample() { match v { Blank => Number(0.) } } }'))
        self.assertFalse(violations('fn read() { /* nested /* Blank => Number(0.) */ comment */ let s = r#"Blank => Number(0.)"#; }'))
        # A production item after a test-only item is still checked.
        self.assertTrue(violations('#[cfg(test)] mod tests {} fn read() { match v { Blank => Number(0.) } }'))


if __name__ == '__main__':
    unittest.main()
