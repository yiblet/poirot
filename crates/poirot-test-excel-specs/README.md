# poirot-test-excel-specs

Workbook behavior specifications and end-to-end tests.

Expected results are supported by recorded sources. Cases cover workbook
editing, calculation, and file operations.

Run from the repository root:

```sh
cargo test -p poirot-test-excel-specs --test workbook_e2e
python3 crates/poirot-test-excel-specs/tools/maintain.py --check
```

When adding cases, record evidence for the expected results rather than deriving
them from Poirot. Include the Excel version and relevant input distinctions,
including blank cells versus empty text or omitted arguments. Modern Excel is
the first compatibility target; known gaps are bugs to close.

This is a development package and is not published.
