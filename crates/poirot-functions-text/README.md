# poirot-functions-text

Text and numeric-text algorithms used by spreadsheet functions, including case
conversion, substring planning, and Roman numeral formatting.

Use `proper` or `roman` for those operations. The `numeric` module handles
numeric-text functions; `units` plans text ranges and reservations. `java_case`
provides full Unicode casing. Case-insensitive comparison uses the shared
`poirot-text::fold` helpers. Hosts evaluate
arguments and enforce resource limits.

Text counting policies live in [poirot-text](../poirot-text/README.md).
