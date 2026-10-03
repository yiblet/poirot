#!/usr/bin/env python3
"""Check typed BIFF writer metadata against the pinned POI source (no JVM)."""
import argparse
import hashlib
from pathlib import Path
import re

SUPPORTED_NAMES_SHA256 = '56bd803a51e5a144b2fa537de4ead9ad5bee91320a6ad29073938850fb03dbd6'
SOURCE_SHA256 = '37687034933e826811fde5eae5eb56c9ff0d630ee4c598370037c3f716a14654'

def parse_source(text):
    rows = {}
    for line in text.splitlines():
        if not line or line.startswith('#'):
            continue
        fields = line.split('\t')
        if len(fields) != 8:
            raise ValueError('metadata row must contain eight columns')
        index, name, minimum, maximum, result, parameters, _, _ = fields
        index, minimum, maximum = int(index), int(minimum), int(maximum)
        result = result or 'R'  # POI parseReturnTypeCode's empty-class contract
        tokens = parameters.split()
        if tokens and tokens[-1] == '...':
            tokens.pop()
        if tokens == ['-']:
            tokens = []
        if (result not in ('R', 'V', 'A') or any(t not in ('R', 'V', 'A') for t in tokens)
                or not 0 <= minimum <= maximum <= 255 or not 0 <= index <= 0x7fff
                or len(tokens) > maximum or (maximum and not tokens)):
            raise ValueError(f'invalid operand metadata for {name}')
        rows[name] = (index, minimum, maximum, result, tuple(tokens),
                      'Last' if len(tokens) < maximum else 'NoTail')
    return rows

def check(source, table):
    data = source.read_bytes()
    if hashlib.sha256(data).hexdigest() != SOURCE_SHA256:
        raise ValueError('POI metadata identity differs from pinned source')
    expected = parse_source(data.decode())
    actual = {}
    pattern = r'"([^"]+)" => \((\d+), (\d+), (\d+), ([RVA]), &\[([RVA, ]*)\], (Last|NoTail)\)'
    for name, index, minimum, maximum, result, parameters, tail in re.findall(pattern, table.read_text()):
        if name in actual:
            raise ValueError(f'duplicate generated name {name}')
        actual[name] = (int(index), int(minimum), int(maximum), result,
                        tuple(t.strip() for t in parameters.split(',') if t.strip()), tail)
    if hashlib.sha256('\n'.join(sorted(actual)).encode()).hexdigest() != SUPPORTED_NAMES_SHA256:
        raise ValueError('writer metadata name set changed without reviewed scope')
    if len(actual) != 269 or any(expected.get(name) != row for name, row in actual.items()):
        names = sorted(actual)
        differences = [name for name in names if actual.get(name) != expected.get(name)]
        raise ValueError(f'typed metadata differs from pinned source: {differences}')
    return len(actual)

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--poi', required=True, type=Path, help='read-only pinned POI checkout')
    args = parser.parse_args()
    source = args.poi / 'poi/src/main/resources/org/apache/poi/ss/formula/function/functionMetadata.txt'
    count = check(source, Path(__file__).resolve().parents[1] / 'src/functions.rs')
    print(f'{count} typed BIFF entries match pinned POI metadata SHA256 {SOURCE_SHA256}')
