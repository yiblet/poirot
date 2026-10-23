# Poirot

Read, edit, calculate, and write Excel workbooks in Rust, without running Excel.

Poirot is for applications that use spreadsheets as calculation models: load a
workbook, change its inputs, and calculate its outputs. It reads and writes XLSX
and XLS files. You can also build a workbook from scratch.

Excel compatibility is incomplete, and the API is under development. Test the
workbooks your application uses. Import warnings report features that were changed
or omitted; saving a file does not guarantee that all of its original features
survive.

## Quick start

Poirot requires Rust 1.90 or newer. Add the Git dependency to `Cargo.toml`:

```toml
[dependencies]
poirot = { git = "https://github.com/yiblet/poirot.git", branch = "master" }
```

Suppose `quote.xlsx` has a sheet named `Quote`, with quantity in `A1`, unit
price in `B1`, and the formula `A1*B1` in `C1`. Change the quantity, calculate the
new total, and save a copy:

```rust
use poirot::Workbook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut book = Workbook::open("quote.xlsx")?;
    let sheet = book.sheet_by_name_mut("Quote").ok_or("missing Quote sheet")?;
    sheet.set_value("A1".parse()?, 12.)?;
    book.recalculate()?;

    let total = book.cell("Quote", "C1".parse()?).ok_or("missing total")?;
    println!("Total: {:?}", total.value);
    book.save_xlsx().write_file("quote-updated.xlsx")?;
    Ok(())
}
```

Saving does not calculate formulas. Call `recalculate()` after changing inputs
when you need current results. Import warnings are available in
`book.file_metadata`; review them before relying on an exported copy.
Use `Workbook::new()` to build a workbook from scratch.

## Examples

- [Edit a file](examples/public_workbook.rs): load a workbook, change an input,
  calculate, and save.
- [Query a model](examples/direct_evaluation.rs): try different inputs without
  changing the original workbook.
- [Save calculation state](examples/snapshot_file.rs): save a checkpoint and
  resume it later.

The file example changes `A1` in the first sheet. Use a workbook where that cell
is an input:

```sh
cargo run --example public_workbook -- input.xlsx output.xlsx
```

Browse the API and task guide with `cargo doc -p poirot --no-deps --open`.
See [Contributing](CONTRIBUTING.md) to work on Poirot.

## Acknowledgements

- [Apache POI](https://poi.apache.org/) shaped Poirot from the start: our first few
  commits translated portions of its code into Rust. Our name is a nod to the
  project. Statistical kernels also adapt
  [Apache Commons Math](https://commons.apache.org/proper/commons-math/).
- [LibreOffice](https://www.libreoffice.org/) provided a foundation for checking
  correctness. Its unit tests helped us understand expected spreadsheet behavior
  and validate our implementation.
- We adapted date handling and regression tests from
  [ClosedXML](https://github.com/ClosedXML/ClosedXML).
  [IronCalc](https://github.com/ironcalc/IronCalc) provided design inspiration.

- [PCRE2](https://github.com/PCRE2Project/pcre2) provides the regex engine through
  the MIT-licensed [Rust bindings](https://github.com/BurntSushi/rust-pcre2).
  See [PCRE2 provenance and notices](LICENSES/PCRE2.md).

See [LICENSES/ACKNOWLEDGEMENTS](LICENSES/ACKNOWLEDGEMENTS) for credits and notices.
Poirot's code is licensed under [MIT](LICENSES/MIT.txt).
