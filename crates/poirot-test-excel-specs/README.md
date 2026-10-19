# poirot-test-excel-specs

Source-backed workbook behavior specifications and their end-to-end test entry
point. Cases cover editing, evaluation, and file workflows, with expected results
supported by recorded evidence.

Run from the repository root:

```sh
cargo test -p poirot-test-excel-specs --test workbook_e2e
python3 crates/poirot-test-excel-specs/tools/maintain.py --check
```

The maintenance check verifies package-local generation and coverage data. When
adding cases, record the evidence for expected results rather than deriving them
from Poirot. This is a development test package and is not published.
