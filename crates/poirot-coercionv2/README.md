# poirot-coercionv2

Shared rules for converting spreadsheet values into function arguments.
Conversions account for input origin, missing arguments, parsing rules, and
resource limits.

Use `Input`, `CoercionV2Context`, and `coercev2_args` with typed requests such as
`DecimalNumber` or `ExactText`. Family modules cover dates, statistics, lookups,
criteria, and other specialized conversions.

Use this crate when implementing function adapters; ordinary workbook callers
can use the [root library](../../README.md).
