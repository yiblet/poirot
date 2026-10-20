# poirot-model

Store and edit workbook data.

`Workbook` holds sparse worksheets, cell values, formulas, named formulas, and
a date system. `Worksheet` provides cell access through zero-based `CellAddress`
coordinates.

A formula cell stores both its formula and a saved result. Changing workbook data
does not calculate a new result. This crate has no file reader or formula evaluator;
use [poirot](https://docs.rs/poirot) for those workflows.
