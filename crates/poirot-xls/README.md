# poirot-xls

Write legacy Excel XLS workbooks.

Use `write_xls` to save the shared workbook model as a BIFF8 compound file.
Use `XlsExportPlan` with `XlsExportPolicy` to check unsupported features and
resource limits before producing output.

Writing preserves saved formula results without recalculating them. XLS input
lives in [poirot-io](https://docs.rs/poirot-io). Most applications should use
[poirot](https://docs.rs/poirot) for the combined workbook workflow.
