# poirot-workbook

Read an Excel workbook, change its inputs, calculate its outputs, and save it.

`Workbook::open` reads XLS or XLSX files. You can also create a workbook in memory:

```rust
use poirot_workbook::Workbook;
use poirot_workbook::model::CellValue;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Quote")?;
    sheet.set_value("A1".parse()?, 12.)?;
    sheet.set_value("B1".parse()?, 25.)?;
    sheet.set_formula("C1".parse()?, "A1*B1")?;
    book.recalculate()?;

    let total = book.cell("Quote", "C1".parse()?).ok_or("missing total")?;
    assert_eq!(total.value, CellValue::Number(300.));
    let bytes = book.save_xlsx().to_buffer()?;
    Ok(())
}
```

Reading a cell returns its stored value. After changing inputs, call `recalculate`
to update dependent cells. Saving writes the current values without calculating.
If calculation fails, it writes none of its results; earlier input edits remain.

Use `from_bytes` to load a buffer. Both `save_xlsx` and `save_xls` let you choose
between returning bytes and writing a file. Import warnings are kept in
`file_metadata`.

For temporary inputs, use `Evaluation` to query results without editing the
workbook. For repeated scenarios, `Overlay` tracks dependencies to reuse
unaffected results. Native snapshots save Poirot state for later restoration;
they are separate from Excel files.

Most applications should depend on [poirot](https://docs.rs/poirot), which
re-exports these types alongside cell values and addresses.
