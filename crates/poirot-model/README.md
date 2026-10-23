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
