# poirot-coercionv2

Convert spreadsheet values into function arguments.

This crate is for function implementations. A conversion request selects the
accepted value types and how to treat missing arguments, text, and cell references.

Use `Input` to describe an argument and `coercev2_args` to convert it. Requests
such as `DecimalNumber` and `ExactText` state what the function accepts. The caller
supplies a `CoercionV2Context` to track conversion work and text allocation.

Most workbook applications should use [poirot](https://docs.rs/poirot).
