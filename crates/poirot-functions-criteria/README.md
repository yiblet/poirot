# poirot-functions-criteria

Match spreadsheet criteria and calculate conditional aggregates.

Use `Criterion` to parse conditions such as `">10"` or `"a*"`. Use `CriteriaPlan`
for aggregates such as `COUNTIF` and `SUMIF`. Wildcard helpers implement
spreadsheet pattern matching; `subtotal` handles hidden rows and nested subtotals.

The caller supplies cells and resolves workbook references.
