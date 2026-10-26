# poirot-model

Store and edit workbook data independently of file handling and calculation.

Use this crate as the data layer when you want to supply your own evaluator or
file adapters, or build a workbook model programmatically.

`Workbook` holds sparse worksheets, cell values, formulas, named formulas, and
a date system. `Worksheet` provides cell access through zero-based `CellAddress`
coordinates.

A formula cell stores both its formula and a saved result. Changing workbook data
does not calculate a new result. This crate has no file reader or formula evaluator;
use [poirot](https://docs.rs/poirot) for those workflows.

`Cell::shared_formula` can carry manually supplied worksheet-local XLSX group provenance.
Current imports materialize shared formulas without this optional metadata.
The complete `Cell::formula` string remains authoritative; the original template
coordinate is provenance, not a dependency on another cell. Formula/value source
setters detach the edited member, while cache/style edits retain membership.
Direct mutable access can leave stale provenance, which writers must validate.
Shared groups are independent of array/spill ownership and evaluator templates.

Construct cell records with constructors or `..Cell::default()` to default optional
metadata. Exhaustive `Cell` struct literals must now include `shared_formula`.
Source snapshot schema 7 stores shared cell styles and workbook theme/palette context;
artifact envelopes reject older source revisions explicitly.

Current XLSX imports do not retain original packages, and saves ignore legacy
`retained_xlsx` bytes carried by a source snapshot.

`CellStyle` keeps number formats alongside an optional `Arc<VisualStyle>` for font,
pattern fill, four borders, and alignment. Use `..CellStyle::default()` when
constructing records. Cloning shares visual payloads; equality compares values.
`FontSize`, `Tint`, `IndexedColor`, and `RotationDegrees` validate numeric fields.
`Workbook::style_context` retains theme bytes and a custom indexed palette without
resolving color identities. These native model fields survive source snapshots;
XLSX visual-style import and export are not yet supported.
