# poirot-io

Read XLS and XLSX workbooks and write XLSX files.

This crate converts workbook files to and from the shared data model. Imports
retain saved formula results and record unsupported or altered content in workbook
metadata. Writing preserves saved results; it does not calculate formulas.
Use this crate when your application owns calculation or only needs file access.

```rust
use poirot_io::{read_workbook_bytes, write_xlsx_to_buffer};
use poirot_model::Workbook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    book.add_sheet("Data")?.set_value("A1".parse()?, 42.)?;
    let bytes = write_xlsx_to_buffer(&book)?;
    let restored = read_workbook_bytes(&bytes)?;
    assert_eq!(restored.sheets().len(), 1);
    Ok(())
}
```

`read_workbook` reads a file; `read_workbook_bytes` reads an in-memory buffer.
Both detect XLS or XLSX from the contents and return the same workbook type.
`ReadOptions` configures import policies and limits.

Review `Workbook::file_metadata` import warnings before relying on an exported
copy. Unsupported content can be changed or omitted; the saved file contains the
features represented by Poirot's model.

Legacy XLS output lives in [poirot-xls](https://docs.rs/poirot-xls).
Most applications should use [poirot](https://docs.rs/poirot), which provides the
same file operations alongside editing and calculation.
