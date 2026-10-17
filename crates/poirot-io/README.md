# poirot-io

Import XLS/XLSX cell data into `poirot_model::Workbook` and export XLSX. File
signatures select the reader independently of filename extensions. Read functions
return `ImportReport` so applications can retain fidelity warnings.

```rust
use poirot_io::{read_xlsx_bytes, write_xlsx_to_buffer};
use poirot_model::Workbook;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S")?.set_value("A1".parse()?, 42.0)?;
    let bytes = write_xlsx_to_buffer(&workbook)?;
    let imported = read_xlsx_bytes(&bytes)?;
    for warning in imported.warnings {
        eprintln!("{warning}");
    }
    Ok(())
}
```

`read_workbook_with_report` opens a file. `ReadOptions` bounds input and ZIP
expansion; BIFF `ResourceBudget` bounds record processing. `OpcImportPolicy`
controls admitted OOXML package/external-link behavior. Export uses
`plan_xlsx_export` with `WriteOptions`; inspect [the export contract](EXPORT.md)
for resource and representation limits.

Loading does not recalculate, and saving preserves current saved formula values
without treating them as trusted evaluator cache provenance. Use `poirot-workbook`
to combine import, editing, calculation and save planning; use `poirot-xls` for XLS
output.

These adapters preserve supported values, formulas, names, date systems and
number formats. They do not preserve arbitrary charts, macros, drawings, rich
styles, validations, merged layout, external links or opaque records. XLS formula
reconstruction and raw number formats have explicit limitations and warnings.
Unsupported numeric/text fidelity can refuse an operation rather than silently
changing a value. Export size limits are not a peak-memory cap, and equal cell
data need not produce byte-identical file metadata.

XLSX array caches carry a bounded rectangle plus dynamic-array metadata on export.
Import restores ownership only for stored children within validated, disjoint array
ranges; it never expands implicit blank coordinates. Blank source formatting is
retained behind cached children. Export rejects array rectangles that would claim
unrelated authored content or conflicting owners. Modern `#SPILL!` and `#CALC!`
values/caches are restored from original XML around the legacy backend decoder.
