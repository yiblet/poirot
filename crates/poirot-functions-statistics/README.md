# poirot-functions-statistics

Statistical calculations for spreadsheet functions.

Includes aggregates, distributions, ranks, percentiles, paired samples,
forecasting, and regression. Entry points include `aggregate`, `percentile`,
and `regression`.

The caller collects numeric inputs and selects the relevant sample policy.
Iterative calculations report convergence failures separately from spreadsheet
errors. Workbook traversal and argument evaluation belong to the caller.
