# poirot-functions-database

Calculate spreadsheet database aggregates.

Use `DatabaseFunction` to select an operation and `DatabasePlan` to apply it to
supplied data and criteria grids. These are the algorithms behind functions such
as `DSUM` and `DCOUNT`, including header matching and row selection.

The caller supplies cells and controls execution limits. No workbook or formula
evaluator is required.
