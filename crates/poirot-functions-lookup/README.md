# poirot-functions-lookup

Lookup and reference-selection algorithms that preserve references when a
function returns one.

Use `evaluate` or `evaluate_with_policy` with the supplied function, argument,
and context types. Implement the `Workbook` port to provide cell reads and
workbook metadata. Lower-level helpers cover classic `MATCH`, `XMATCH`,
approximate search, `INDEX` selection, and dynamic-reference parsing.
