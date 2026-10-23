# poirot-io

Read XLSX with MIT-licensed Calamine streaming cells; read XLS and write fresh XLSX
with MIT-licensed rxls.

Calamine streams XLSX cells directly into Poirot, avoiding a second full workbook
model. Poirot recovers number formats, names, date system and presentation.
Original shared group IDs and anchors are not retained. Imported formulas remain
independent through edits and calculation, and the compiler can still share its
executable templates. Optional manually supplied shared provenance is checked on
export before formula text is omitted.

Two existing XML recovery operations protect supported calculation behavior:
exact Excel UTF-16 text, XML line endings and missing/empty caches; and array/spill
ownership so saved derived cells cannot become authored blockers. The first pass
also preserves sparse physical rows, row visibility and cell number formats. The spill pass runs only when
the first pass encounters array formulas or modern spill/calculation errors.
There is no separate original shared-group validation or reconstruction pass.

Fresh authoring uses `rxls::Workbook::to_xlsx_checked`; XML projection restores
canonical saved values, typed caches and array metadata without recalculation.

This crate converts workbook files to and from the shared data model. Imports
retain saved formula results and record unsupported or altered content in workbook
metadata. Writing preserves saved results; it does not calculate formulas.

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

XLS is supported as input only; saves produce XLSX.
Most applications should use [poirot](https://docs.rs/poirot), which provides the
same file operations alongside editing and calculation.

Saves always author a fresh XLSX package from the modeled values, formulas, cached
results, date system, names and number formats. See the
[compatibility guide](https://github.com/yiblet/poirot/blob/master/docs/compatibility.md)
for preservation limits. Original shared encoding and unknown formula attributes
are not preserved. Supported structural edits do not encounter a retained-package gate.
Legacy `Workbook::retained_xlsx` bytes can still pass through native snapshots,
but imports leave that field empty and exports ignore it.

The remaining adapter work has a specific purpose:

- OPC/ZIP and CFB/BIFF admission enforce configured limits and package identity.
  Import checks the ZIP catalog before decoding. Calamine streams XLSX cells;
  after releasing the reader, recovery expands shared strings and one worksheet
  at a time, reusing its bytes for conditional array recovery. Ordinary XLSX bytes
  go directly to Calamine. A transient stored ZIP is built only for unsupported
  metadata paths, modern errors or foreign cell children; decoding restarts from
  that adapter rather than resuming a partially consumed cell. XLS decoding is sparse.
- XLSX text recovery preserves values, formula source and cache distinctions.
  Array recovery preserves calculation ownership, not untouched feature layout.
- BIFF recovery restores raw UTF-16 strings, formula tokens, scoped names and sheet
  identity that backend APIs cannot fully represent. Original bytes go directly
  to rxls without a compound-file clone.
- XLSX export projection writes canonical numeric values, text, typed caches,
  source metadata and dynamic-array records.

Shared formulas use Calamine's metadata streaming API and public translation
helper with a sparse master map and the actual master coordinate. Forward
followers are resolved after the worksheet stream; huge sparse shared indices do
not allocate a dense table. Original shared encoding is not retained. Malformed
shared ranges and duplicate masters are not separately validated. The three reserved
print-area, print-title and filter-database names are omitted because those features
are not modeled; other defined-name identities remain intact.
Raw cell-XF number formats, including styled blanks, are recovered during the
existing text pass; unsupported built-in formats and rich styling may be lost.
