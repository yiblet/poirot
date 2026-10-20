# poirot-functions-core

Cell values and deferred arguments for spreadsheet functions.

`CellValue` represents a scalar value, including spreadsheet errors and Excel
text. `Origin` distinguishes literal arguments from values read through references.

Implement `Input` to let a function evaluate an argument only when it needs it.
These types let function algorithms run without a workbook or formula evaluator.
Workbook storage lives in [poirot-model](https://docs.rs/poirot-model).
