"""Generate Character simple maps from pinned OpenJDK21.0.8 UnicodeData only.
Usage: python3 generate_java_simple_case.py UnicodeData.txt output.bin
SpecialCasing is intentionally excluded. Unicode notice accompanies output.
"""
import hashlib
from pathlib import Path
import struct
import sys

SOURCE_SHA256 = '806e9aed65037197f1ec85e12be6e8cd870fc5608b4de0fffd990f689f376a73'
source = Path(sys.argv[1]).read_bytes()
assert hashlib.sha256(source).hexdigest() == SOURCE_SHA256, 'pinned input drift'
rows = []
for line in source.decode('utf-8').splitlines():
    fields = line.split(';')
    cp = int(fields[0], 16)
    upper = int(fields[12], 16) if fields[12] else cp
    lower = int(fields[13], 16) if fields[13] else cp
    if (lower, upper) != (cp, cp):
        rows.append((cp, lower, upper))
assert rows == sorted(rows)
data = b'ROIS15\0\0' + struct.pack('<I', len(rows))
data += b''.join(struct.pack('<III', *row) for row in rows)
Path(sys.argv[2]).write_bytes(data)
