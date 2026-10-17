# poirot

Read, edit, evaluate and write Excel workbooks in Rust. Change inputs, calculate
formula results, and save the updated workbook as XLSX or XLS. For repeated
what-if calculations, overlays keep edits separate from the original workbook
and reuse unaffected results.

Formula support and file preservation are incomplete. See
[compatibility and limitations](#compatibility-and-limitations) before using
Poirot to update existing files.

## Quick start

Add Poirot from this repository to your project's `Cargo.toml`:

```toml
[dependencies]
poirot = { git = "https://github.com/yiblet/poirot.git" }
```

To try it from a checkout without supplying an Excel file, run:

```sh
cargo run --example public_workbook
```

That example creates a workbook, changes an input, calculates a formula, and
checks that the result survives saving and reopening both XLSX and XLS in memory.

## Open, edit, calculate and save

Suppose `input.xlsx` has a sheet named `Inputs`, with a number in `A1` and the
formula `A1*2` in `B1`. This program changes `A1` to 24, calculates `B1` as 48,
and saves the updated workbook to `output.xlsx`.

```rust,no_run
use poirot::{CellValue, Evaluation, EvaluationContext, Spreadsheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Spreadsheet::open("input.xlsx")?;
    for warning in book.import_warnings() {
        eprintln!("Import warning: {warning}");
    }

    let changes = {
        let mut evaluation = Evaluation::with_workbook(book.workbook())
            .build(EvaluationContext::default())?;
        let input = evaluation.target("Inputs!A1")?;
        let output = evaluation.target("Inputs!B1")?;

        evaluation.set_value(input, 24.0)?;
        evaluation.all()?;
        if evaluation.cell(output)? != CellValue::Number(48.0) {
            return Err("unexpected evaluation result".into());
        }
        evaluation.into_changes()
    };

    book.workbook_mut().apply(changes)?;
    book.plan_xlsx_save(Default::default())?
        .write_file("output.xlsx")?;
    Ok(())
}
```

`Spreadsheet` holds the workbook and its import warnings. An `Evaluation` borrows
the workbook and keeps edits and calculated results separate until you apply
its changes. The block ends that borrow so the workbook can be updated.

Use `cell(target)` for one result, `outputs(&targets)` for a list, or `all()` to
calculate all stored cells and discovered spill children. The example uses
`all()` to refresh formula results throughout the workbook before saving.
**Saving never recalculates:** evaluating only selected outputs leaves unrelated
saved formula results unchanged. Use `plan_xls_save` to save as XLS instead.

## Choose an evaluation mode

Start with `Evaluation` for a single calculation or edit-and-save workflow. Use
`Overlay` when you repeatedly change inputs and want to retain unaffected results.

| | `Evaluation` | `Overlay` |
| --- | --- | --- |
| Calculate results | Call `cell`, `outputs` or `all` directly | Call them through `overlay.evaluation(&context)?` |
| After an edit | Clears all local calculated results | Invalidates the changed cells and their dependents |
| Reuse between calculations | Retains compiled formulas | Retains compiled formulas, dependencies and valid results |
| Apply edits and results | Export with `changes()` or `into_changes()` | Export with `changes()` or `into_changes()` |

Both modes use the same formula engine and support changing values, formulas,
styles, and removing cells. Neither copies the baseline workbook's cells.
Pass an `Arc<Workbook>` to share a baseline across independently owned workspaces.

`EvaluationContext` controls time, random seed, localization, and execution
budgets. Reuse the same context inputs for reproducible time and random values.
See the [API guide](docs/public-api.md) for run lifetimes and configuration,
and [dependency tracking](docs/incremental-cache.md) for how overlays reuse results.

## Compatibility and limitations

| Area | Current behavior |
| --- | --- |
| File formats | Reads and writes XLSX and XLS, with format-specific limits. |
| Formula support | Incomplete; unknown functions return `#NAME?`. |
| File preservation | Preserves supported values, formulas, names, date systems and number formats. Arbitrary charts, macros, drawings, rich styles, validations, merged layout and external links are not preserved. |
| Import diagnostics | Inspect `Spreadsheet::import_warnings()` for features omitted or changed during import. |
| Calculation errors | Spreadsheet errors are `CellValue::Error`; operational failures are returned as Rust errors. |

See the [I/O guide](crates/poirot-io/README.md),
[XLSX export contract](crates/poirot-io/EXPORT.md), and
[XLS guide](crates/poirot-xls/README.md) for supported representations and limits.

## More examples and documentation

| Task | Start here |
| --- | --- |
| Create, edit, calculate and round-trip a workbook | [Public workflow](examples/public_workbook.rs) |
| Evaluate over a shared workbook | [Direct evaluation example](examples/direct_evaluation.rs) |
| Configure evaluation and apply changes | [API guide](docs/public-api.md) |
| Understand incremental recalculation | [Dependency tracking](docs/incremental-cache.md) |
| Work with dynamic array results | [Dynamic arrays](crates/poirot-eval/docs/dynamic-array-results.md) |
| Save and restore Poirot state | [Snapshot files](docs/snapshot-files.md) and [runnable example](examples/snapshot_file.rs) |

Snapshots persist workbook data and optional prepared evaluation state separately
from Excel files. Use `poirot::artifact` for versioned snapshot files; see the guide
for compatibility checks, decoding limits, and trusted-state requirements.

## Development

From the repository root:

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Use `--workspace` to test every crate; plain `cargo test` selects the root
package, FODS runner, and [JSON scenario runner](crates/poirot-test-json/README.md).
Generate the Rust API documentation with
`cargo doc -p poirot --no-deps --open`.

The component crates cover [workbook data](crates/poirot-model/README.md),
[formula syntax](crates/poirot-formula/README.md),
[evaluation](crates/poirot-eval/README.md),
[formatting](crates/poirot-format/README.md), file I/O, and
[workbook integration](crates/poirot-workbook/README.md).
