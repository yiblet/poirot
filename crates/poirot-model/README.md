# poirot-model

The shared workbook data model: sparse worksheets, typed cell values, formulas,
number formats, named ranges, and owned changes.

Use `Workbook`, `Worksheet`, `CellAddress`, and `WorkbookChange` to build or inspect
workbook state without loading files or running formulas. Formula cells retain
saved results; changing the model does not calculate new ones.

For file handling and calculation, start with the [root library](../../README.md).
