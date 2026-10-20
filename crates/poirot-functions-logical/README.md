# poirot-functions-logical

Scalar helpers for spreadsheet logical functions.

Use `coercev2_logical_test` to convert a value into a logical condition and
`error_type` to obtain an `ERROR.TYPE` code.

The caller selects branches and evaluates arguments, including short-circuit
behavior. This crate does not evaluate complete formulas.
