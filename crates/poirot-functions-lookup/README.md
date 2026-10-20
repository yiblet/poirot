# poirot-functions-lookup

Look up spreadsheet values and select references.

Use `evaluate` with a selected function and its arguments. Implement the
`Workbook` trait to supply cell reads and workbook metadata. Results retain
references when the spreadsheet function returns one.

Lower-level helpers cover `MATCH`, `XMATCH`, approximate search, `INDEX`, and
parsing dynamic references. The caller evaluates arguments and controls limits.
