# poirot-functions-dates

Calculate calendar intervals with spreadsheet DATEDIF rules.

Call `datedif` with two date serials, a `DateIntervalUnit`, and the workbook's
date system. It calculates complete years, months, days, and supported remainder
units, including the implemented Excel calendar quirks.

Date parsing and argument conversion live in
[poirot-coercionv2](https://docs.rs/poirot-coercionv2).
