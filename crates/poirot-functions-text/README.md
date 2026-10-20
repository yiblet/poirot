# poirot-functions-text

Text calculations for spreadsheet functions.

Includes title casing with `proper`, Roman numerals with `roman`, numeric-text
operations, and substring planning. The caller evaluates arguments and controls
allocation limits.

`java_case` implements Unicode casing. Case-insensitive comparison uses
[poirot-text](https://docs.rs/poirot-text), which also defines text counting and
slicing rules.
