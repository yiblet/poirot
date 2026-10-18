# Create and calculate a workbook

[Documentation](README.md) · Next: [Evaluation and overlays](evaluation.md)

Add Poirot to your project's `Cargo.toml`:

```toml
[dependencies]
poirot = { git = "https://github.com/yiblet/poirot.git" }
```

This example creates a formula, changes its input, calculates the result, and
saves an XLSX file. It needs no existing Excel file.

```rust,no_run
use poirot::{CellValue, Evaluation, EvaluationContext, Spreadsheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Spreadsheet::new();
    let sheet = book.workbook_mut().add_sheet("Inputs")?;
    sheet.set_value("A1".parse()?, 21.0)?;
    sheet.set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;

    let changes = {
        let mut evaluation = Evaluation::with_workbook(book.workbook())
            .build(EvaluationContext::default())?;
        let input = evaluation.target("Inputs!A1")?;
        let output = evaluation.target("Inputs!B1")?;
        evaluation.set_value(input, 24.0)?;
        println!("{:?}", evaluation.cell(output)?); // Number(48.0)
        evaluation.all()?;
        evaluation.into_changes()
    };

    book.workbook_mut().apply(changes)?;
    book.plan_xlsx_save(Default::default())?
        .write_file("output.xlsx")?;
    Ok(())
}
```

`Workbook` owns the source cells. `Spreadsheet` wraps it with file operations and
import warnings. `Evaluation` borrows the workbook and holds edits and calculated
results until you export and apply them. The block ends the immutable borrow
before `workbook_mut()` updates the source.

The initial `CellValue::Blank` passed to `set_formula` is a saved result, not a
request to calculate. Evaluation computes from the formula and current inputs.
`all()` refreshes all stored cells and discovered spill children before saving;
calculating only `B1` would leave unrelated formula caches unchanged.

To start from a file, replace `Spreadsheet::new()` and source construction with
`Spreadsheet::open("input.xlsx")?`, and inspect `book.import_warnings()`.
See [Excel files](excel-files.md) before modifying existing workbooks.

For an executable version that also reopens XLSX and XLS output, run
`cargo run --example public_workbook` from the repository root.
