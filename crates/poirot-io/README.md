# poirot-io

Read XLSX with MIT-licensed Calamine streaming cells, read BIFF8 XLS with Poirot's
own sparse decoder, and write fresh XLSX with rust_xlsxwriter (used under MIT).

Calamine streams XLSX cells directly into Poirot, avoiding a second full workbook
model. Poirot recovers names, date system and ordinary XLSX cell presentation:
number formats, fonts, pattern fills, four-sided borders and common alignment.
Styles share immutable `VisualStyle` payloads; `Workbook::style_context` retains
the default style, theme bytes and indexed palette. Colors retain their source
identities rather than resolved RGB values. XLS imports recover neither number
formats nor visual styles.
Original shared group IDs and anchors are not retained. Imported formulas remain
independent through edits and calculation, and the compiler can still share its
executable templates. Optional manually supplied shared provenance is checked on
export before formula text is omitted.

Two existing XML recovery operations protect supported calculation behavior:
exact Excel UTF-16 text, XML line endings and missing/empty caches; and array/spill
ownership so saved derived cells cannot become authored blockers. The first pass
also preserves sparse physical rows, row visibility and cell styles. The spill pass runs only when
the first pass encounters array formulas or modern spill/calculation errors.
There is no separate original shared-group validation or reconstruction pass.

Fresh authoring uses rust_xlsxwriter for the package skeleton (sheets, order,
visibility, tab colors and used ranges); XML projection writes canonical cells,
saved values, typed caches, styles, names and array metadata without
recalculation. Only Poirot's `WriteOptions` limit output size.

This crate owns file records and codecs. Implement `data::WorkbookSink` for direct
streaming import and `data::WorkbookSource` for borrowed export; Poirot implements
both for its sparse model. Imports
retain saved formula results and record unsupported or altered content in workbook
metadata. Writing preserves saved results; it does not calculate formulas.

```rust
use poirot_io::{IoError, read_workbook_bytes, write_xlsx_to_buffer};
use poirot_io::data::{SheetSink, WorkbookSink};

// The caller supplies storage; import writes directly into that storage type.
fn roundtrip<W: WorkbookSink<Sheet: SheetSink>>(book: &W) -> Result<W, IoError> {
    let bytes = write_xlsx_to_buffer(book)?;
    read_workbook_bytes(&bytes)
}
```

`read_workbook` reads a file; `read_workbook_bytes` reads an in-memory buffer.
Both detect XLS or XLSX from the contents and construct the caller-selected sink.
`ReadOptions` configures import policies and limits.

XLS is supported as input only; saves produce XLSX.
Most applications should use [poirot](https://docs.rs/poirot), which provides the
same file operations alongside editing and calculation.

Saves always author a fresh XLSX package from the modeled values, formulas, cached
results, date system, names and supported cell styles. See the
[compatibility guide](https://github.com/yiblet/poirot/blob/master/docs/compatibility.md)
for preservation limits. Original shared encoding and unknown formula attributes
are not preserved.

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
- BIFF8 decoding reads sparse cell records, exact UTF-16 strings, formula tokens
  (including shared formulas), scoped names, sheet identity and the date system.
  Unsupported formula token families keep saved values and add an import warning.
  BIFF5 and earlier, and encrypted workbooks, are refused.
- XLSX export projection writes canonical numeric values, text, typed caches,
  source metadata and dynamic-array records.

Shared formulas use Calamine's metadata streaming API and public translation
helper with a sparse master map and the actual master coordinate. Forward
followers are resolved after the worksheet stream; huge sparse shared indices do
not allocate a dense table. Original shared encoding is not retained. Malformed
shared ranges and duplicate masters are not separately validated. The three reserved
print-area, print-title and filter-database names are omitted because those features
are not modeled; other defined-name identities remain intact.
Raw cell-XF styles, including styled blanks, are recovered during the existing
text pass. Unsupported features such as conditional formatting, gradient fills,
diagonal borders and row/column style inheritance are not preserved; inspect
import warnings. Layout and font rendering are outside this crate's scope.
