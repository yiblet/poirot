# poirot-functions-statistics

Statistical calculations for spreadsheet functions.

Includes aggregates, distributions, ranks, percentiles, paired samples,
forecasting, and regression. Entry points include `aggregate`, `percentile`,
and `regression`.

The caller collects numeric inputs and selects the relevant sample policy.
Iterative calculations report convergence failures separately from spreadsheet
errors. Workbook traversal and argument evaluation belong to the caller.

The `ets` module prepares regular numeric or calendar timelines and fits a bounded
additive innovations model for the FORECAST.ETS family. See
[src/ETS_SOURCES.md](src/ETS_SOURCES.md) for documented contracts, independent
controls, and numerical choices that have not yet been verified against native
Excel. Registration does not imply identical optimizer or diagnostic conventions.
