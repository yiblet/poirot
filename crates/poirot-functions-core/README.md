# poirot-functions-core

Workbook-independent values and input contracts shared by the function crates.

Provides `CellValue`, `CellError`, exact Excel text storage, and scalar inputs
that retain their origin. The `Input` trait lets algorithms request deferred
arguments without depending on a workbook or evaluator.

Use these types when adding reusable function algorithms or implementing a host
for them. Workbook structures live in [poirot-model](../poirot-model/README.md).
