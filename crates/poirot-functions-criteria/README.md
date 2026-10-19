# poirot-functions-criteria

Criteria matching and conditional aggregation for functions such as `COUNTIF`
and `SUMIF`, plus `SUBTOTAL` rules for hidden rows and nested subtotals.

Use `Criterion` to parse and test a criterion, or `CriteriaPlan` to prepare an
aggregation over supplied ranges. Wildcard helpers support spreadsheet patterns.
The caller resolves workbook references and supplies the input cells.
