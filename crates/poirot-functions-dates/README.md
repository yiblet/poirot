# poirot-functions-dates

The workbook-independent `DATEDIF` calendar interval algorithm.

Use `datedif` with admitted date serials, a `DateIntervalUnit`, and the workbook's
calendar epoch. It computes complete years, months, days, and the supported
remainder units while preserving the implemented Excel calendar quirks.

Date parsing and argument conversion live in
[poirot-coercionv2](../poirot-coercionv2/README.md).
