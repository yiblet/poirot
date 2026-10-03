"""Generate ROI's immutable Unicode15 mapping asset; no runtime oracle execution.
Usage: python3 generate_java_case.py UnicodeData.txt SpecialCasing.txt output.bin
Use the pinned OpenJDK 21.0.8 Unicode 15 inputs; retain the Unicode notice.
"""
from pathlib import Path
import struct
import sys
rows = {}
for line in Path(sys.argv[1]).read_text().splitlines():
    fields = line.split(';')
    cp = int(fields[0], 16)
    lower = [int(fields[13], 16)] if fields[13] else [cp]
    upper = [int(fields[12], 16)] if fields[12] else [cp]
    if lower != [cp] or upper != [cp]:
        rows[cp] = [lower, upper]
for line in Path(sys.argv[2]).read_text().splitlines():
    fields = line.split('#')[0].strip().split(';')
    if len(fields) < 5 or fields[4].strip():
        continue
    cp = int(fields[0], 16)
    rows[cp] = [[int(x, 16) for x in fields[1].split()],
                [int(x, 16) for x in fields[3].split()]]
data = b'ROIUC15\0' + struct.pack('<I', len(rows))
for cp, (lower, upper) in sorted(rows.items()):
    assert 1 <= len(lower) <= 3 and 1 <= len(upper) <= 3
    data += struct.pack('<IBB6I', cp, len(lower), len(upper),
                        *(lower + [0] * (3-len(lower))),
                        *(upper + [0] * (3-len(upper))))
Path(sys.argv[3]).write_bytes(data)
