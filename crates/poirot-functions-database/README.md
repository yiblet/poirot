# poirot-functions-database

Database-style spreadsheet aggregation over tables with headers and criteria,
including the algorithms behind the D-function family.

Use `DatabaseFunction`, `Grid`, and `DatabasePlan` to select an operation and
prepare it over supplied data and criteria. The crate handles row selection and
aggregation; the caller supplies workbook data and controls execution resources.
