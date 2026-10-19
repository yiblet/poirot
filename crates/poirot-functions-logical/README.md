# poirot-functions-logical

Small scalar helpers for logical functions: converting a value to a logical
test and mapping spreadsheet errors to `ERROR.TYPE` codes.

Use `coercev2_logical_test` and `error_type` when implementing logical function
adapters. Branch selection, short-circuit evaluation, and workbook reads remain
with the host; this crate does not implement a standalone formula evaluator.
