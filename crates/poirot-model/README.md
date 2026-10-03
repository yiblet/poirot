# poirot-model

Sparse workbook data independent of evaluation and file formats. `Workbook`
contains ordered sheets, scoped defined names and a date system. `Worksheet`
stores only present cells and rows. Missing cells differ from stored blanks.

```rust
use poirot_model::{CellValue, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = Workbook::new();
    let sheet = workbook.add_sheet("Inputs")?;
    sheet.set_value("A1".parse()?, 21.0)?;
    sheet.set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;
    assert_eq!(sheet.get("B1".parse()?).unwrap().value, CellValue::Blank);
    Ok(())
}
```

Setting a formula stores its source and supplied saved value; it does not evaluate
it. `CellAddress::new(row, col)` uses zero-based coordinates. Parsing A1 spelling
uses one-based row/column names and validates Excel worksheet bounds.

Values preserve numbers, booleans, typed spreadsheet errors and text. Exact
UTF-16 text can contain isolated surrogates; use fallible Unicode projections
when scalar text is required. Dates remain numeric serials under the workbook's
`DateSystem`. `CellStyle::number_format` retains a raw Excel format string;
arbitrary rich workbook presentation is outside the model.

Use `poirot-workbook` for formula-aware edit transactions and composed calculation
and save workflows. [Sparse queries](docs/SPARSE_QUERIES.md) explain bounded
range access and row presence.

`Workbook::snapshot()` returns a borrowed `snapshot::Snapshot<'_>` implementing
Serde; `Workbook` itself does not implement Serde. Deserialize an owned snapshot
and call `Workbook::from_snapshot`. Numeric bits, UTF-16 units, sparse rows, styles,
names and derived spill ownership survive the round trip.

`WorkbookChange` is an owned, serializable mutation enum. `Workbook::apply` applies
an entire batch atomically. Literal, formula, saved-result and derived-spill writes
have distinct variants. Setting a literal clears a formula; setting its cached
value preserves and checks the formula source. Spill children retain their prior
authored source cell, so clearing derived output cannot delete original styling.

`CellStore<T, HEIGHT, WIDTH>` shares sparse tiled storage across source cells and
other payloads. Default workbook tiles are 32x4; overlays can use 16x2. Ordered,
column and occupied-row queries use the same implementation for every layout.
`CellRead<T>` supplies the common borrowed read interface without requiring
worksheet metadata to be copied into an overlay.
