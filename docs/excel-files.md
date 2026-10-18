# Open and save Excel files

[Documentation](README.md)

`Spreadsheet` combines a workbook with import warnings and checked save planning.
Use `open(path)` for a file or `from_bytes(&bytes)` for in-memory input. File
signatures select XLSX or XLS independently of the filename extension.

```rust,no_run
use poirot::{Evaluation, EvaluationContext, Spreadsheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Spreadsheet::open("input.xlsx")?;
    for warning in book.import_warnings() {
        eprintln!("Import warning: {warning}");
    }
    let changes = {
        let mut evaluation = Evaluation::with_workbook(book.workbook())
            .build(EvaluationContext::default())?;
        evaluation.all()?;
        evaluation.into_changes()
    };
    book.workbook_mut().apply(changes)?;
    book.plan_xlsx_save(Default::default())?
        .write_file("output.xlsx")?;
    Ok(())
}
```

## Calculate before saving

Import and export do not calculate formulas. Imported saved results are retained
file data, not proof that the evaluator can reuse them. Evaluate the required
outputs, apply the resulting changes, and then save.

Use `all()` to refresh every modeled stored cell and discovered spill child.
Selected `cell()` or `outputs()` calls leave unrelated saved formula results
unchanged. Formula errors are values; propagate operational failures before
publishing a file.

`plan_xlsx_save(options)` and `plan_xls_save(policy)` validate the workbook and
produce a borrowed `SavePlan`. Call `to_buffer()` for bytes or `write_file(path)`
for a file. Complete workbook edits before creating a save plan.

## Understand preservation limits

| Data | Support |
| --- | --- |
| Values, formulas, names, date systems, number formats | Supported within format-specific limits |
| Charts, macros, drawings, rich styles, validations, merged layout, external links | Arbitrary original features are not preserved |
| Exact text and numeric representations | Checked; unsupported fidelity can refuse an operation |
| Runtime dynamic-array ownership | Not completely encoded by XLS/XLSX adapters |

Inspect import warnings before accepting a round trip. Export reconstructs the
supported workbook model; it cannot recover features omitted during import.
Equal cell data does not imply byte-identical files. XLS has additional formula
and representation restrictions.

For detailed contracts, see [file import](../crates/poirot-io/README.md),
[XLSX export](../crates/poirot-io/EXPORT.md), and [XLS](../crates/poirot-xls/README.md).
Lower-level `ReadOptions`, BIFF resource budgets and export policies provide
format-specific limits; those limits are not process-wide peak-memory caps.

Direct file writes do not provide atomic replacement. Applications choose their
own publication and durability policy, for example writing a temporary file and
replacing the destination only after success.

Use [snapshot files](snapshots.md) to store Poirot source and optional prepared
state between runs. Snapshots and Excel files serve different persistence needs.
