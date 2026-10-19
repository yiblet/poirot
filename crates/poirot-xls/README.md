# poirot-xls

Write the shared workbook model to legacy BIFF8 `.xls` files.

Use `XlsExportPlan` with `XlsExportPolicy` to check representation and resource
limits before writing, or `write_xls` for the default workflow. Unsupported
formulas and features can be refused during planning. Export preserves saved
formula results without recalculating them.

XLS import lives in [poirot-io](../poirot-io/README.md).
