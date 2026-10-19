# Poirot

Read, edit, evaluate, and write Excel workbooks in Rust.

Poirot brings file handling and formula evaluation together, so you can automate
workbook changes and calculate results without running Excel.

- Read and write `.xlsx` and `.xls` files.
- Create sheets, edit cells, and evaluate formulas.
- Explore different inputs in an overlay before applying changes to a workbook.
- Save and restore evaluation state with snapshots.

Evaluation defaults target modern Excel: documented builtin behavior, surrogate-pair
text indexing (Compatibility Version 2), and dynamic array spills. Explicit source
and scalar-intersection policies remain available for compatibility comparisons.
Case-insensitive value comparisons use Unicode 16 simple folding; full casing
for `UPPER` and `LOWER` uses Unicode 15.

Poirot is intended for Rust backends that embed and validate controlled workbook
models. It is under active development: Excel compatibility is incomplete, and the
API may change. Legacy array formulas and some function argument shapes remain
unsupported. Review import warnings and test your workbooks to check what survives
loading and saving.

## Quick start

Add Poirot to your `Cargo.toml` using the `master` branch:

```toml
[dependencies]
poirot = { git = "https://github.com/yiblet/poirot.git", branch = "master" }
```

Create a workbook, calculate `B1` as `42`, and save it to `output.xlsx`:

```rust
use poirot::{CellValue, EvaluationContext, Overlay, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::new();
    let sheet = book.add_sheet("Inputs")?;
    sheet.set_value("A1".parse()?, 21.)?;
    sheet.set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;

    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&book).build()?;
        overlay.evaluation(&context)?.all()?;
        overlay.changes()
    };
    book.apply(changes)?;

    book.save_xlsx().write_file("output.xlsx")?;
    Ok(())
}
```

To load an existing workbook, use `Workbook::open("input.xlsx")?`, then review
`book.file_metadata` for features changed or omitted during import. Saving
preserves the current results. To update them, evaluate formulas and apply the
changes before saving, as shown above.

## Examples and API

- [Workbook workflow](examples/public_workbook.rs): edit cells, calculate results,
  and save to XLSX or XLS.
- [Direct evaluation](examples/direct_evaluation.rs): change inputs and query
  formula results.
- [Snapshots](examples/snapshot_file.rs): save evaluation state and resume later.

From the repository root, run the workbook example or browse the API documentation:

```sh
cargo run --example public_workbook
cargo doc -p poirot --no-deps --open
```

## Development

Contributions are welcome! See [Contributing](CONTRIBUTING.md) to get started.

The supported Rust baseline is 1.99. Run tests, formatting checks, and lints from the
repository root:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Acknowledgements

- [Apache POI](https://poi.apache.org/) shaped Poirot from the start: our first few
  commits translated portions of its code directly into Rust. Our name is a nod
  to the project, too. Statistical kernels also adapt
  [Apache Commons Math](https://commons.apache.org/proper/commons-math/).
- [LibreOffice](https://www.libreoffice.org/) provided a foundation for checking
  correctness. Its unit tests helped us understand expected spreadsheet behavior
  and validate our implementation.
- [ClosedXML](https://github.com/ClosedXML/ClosedXML) and
  [IronCalc](https://github.com/ironcalc/IronCalc) offered additional inspiration
  for our approach to building a spreadsheet library.

First-party code is [MIT licensed](LICENSE). See [NOTICE](NOTICE) for third-party
licenses and attribution.
