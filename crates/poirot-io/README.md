# poirot-io

Read XLS/XLSX workbooks into the shared model and write XLSX files.

Use `read_workbook_with_report` to retain import warnings, or the byte-based
readers for in-memory input. `plan_xlsx_export` validates a workbook before
serialization. Read and write options control resource limits and supported
features; inspect diagnostics when checking file fidelity.

XLS export lives in [poirot-xls](../poirot-xls/README.md). For a combined workflow,
use `Spreadsheet` from the [root library](../../README.md).
