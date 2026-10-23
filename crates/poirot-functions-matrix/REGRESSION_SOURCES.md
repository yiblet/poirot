# Regression contracts and numerical policy

GROWTH, LINEST and LOGEST are independently implemented from documented worksheet
contracts and least-squares mathematics. No POI, Commons Math or other library
source, algorithms expressed as source, tests or data were copied or translated.
The existing pinned TREND path remains separate. The modern pure TREND API and
evaluator use the same scaled, column-pivoted Householder least-squares kernel.

Primary contracts:

- [LINEST](https://support.microsoft.com/en-us/excel/functions/linest-function):
  reverse predictor coefficient order, intercept choice and five statistics rows.
- [Office LINEST clarifications](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/a7659bd2-9515-448d-a9da-2f14ee87f3ab):
  dimension errors, zero coefficients/errors for removed predictors, and NA for
  the forced intercept's standard error.
- [VBA LINEST](https://learn.microsoft.com/en-us/office/vba/api/Excel.WorksheetFunction.LinEst):
  removed predictors increase residual degrees of freedom.
- [LOGEST](https://support.microsoft.com/en-us/excel/functions/logest-function)
  and [GROWTH](https://support.microsoft.com/en-us/excel/functions/growth-function):
  fit logarithmic positive responses and exponentiate coefficients/predictions.
- [GROWTH numerical discussion](https://learn.microsoft.com/en-us/troubleshoot/microsoft-365-apps/excel/statistical-functions-growth):
  QR relationship between LINEST, LOGEST and GROWTH; a removed exponential
  predictor becomes multiplier one.
- [TREND](https://support.microsoft.com/en-us/excel/functions/trend-function):
  observation axes and default known/new predictors.

The kernel subtracts a first-observation anchor before scaling and centering
intercept fits; through-zero fits scale without centering. Each predictor is
scaled independently. At every QR step the largest remaining trailing column
norm wins (first position breaks exact ties). Rank stops at
`16 * EPSILON * max(observations, predictors) * initial_pivot_norm`.
This deterministic relative policy, the chosen redundant predictor, and
underdetermined coefficient selection have not been verified against native
Excel. No claim of exact native rank tolerance is made.

Slopes, predictions, rank and optional diagnostics share one fit. Predictions
skip exact-zero slopes before normalizing irrelevant new coordinates. A bounded
fallback projects slopes into original units if extrapolation overflows only
in normalized coordinates; the common path retains centered evaluation. Diagnostics
solve the retained triangular factor for covariance, calculate residuals directly
in normalized coordinates, and use residual degrees `n - rank - intercept`.
For through-zero fits total squares are uncentered. A negative explained sum
caused by finite subtraction is bounded below at zero: an exact least-squares
projection cannot have residual energy above total energy. Zero total squares,
zero residual degrees, or undefined F statistics produce individual DIV0 cells;
unused statistics cells and the forced intercept error are NA. LOGEST statistics
remain in log-response units. Coefficients can be available while diagnostics
are undefined. These degenerate cell-error choices are declared policy pending
native confirmation, not inferred from Poirot output.

Collections retain every original position and require actual numbers, propagating
embedded spreadsheet errors. Omitted optional operands use defaults; explicit
blank collections are not treated as omitted. Const/stats accept scalar Boolean
or numeric coercion; missing options use documented defaults. Detailed native
blank/text/logical-origin precedence remains an audit item. No row is compacted
independently. The modern adapter now accepts reference Boolean options, unlike
the preserved pinned TREND adapter.

Storage is O(np + p² + mp), fit work O(np min(n,p)), optional diagnostics O(p³),
and predictions O(mp), with checked estimates admitted before the fit/transpose.
Allocation refusals and host budgets remain operational, outside IFERROR.
Stats FALSE skips covariance work.

Tests derive exact systems independently. For x=(0,1,2), y=(1,2,2), slope=1/2,
intercept=7/6, SSE=1/6, SSR=1/2, df=1, r²=3/4, sey=sqrt(1/6), F=3,
slope SE=sqrt(1/12), intercept SE=sqrt(5)/6. Through-zero x=(1,2), y=(1,1)
gives slope=3/5, SSE=1/5 and uncentered r²=9/10. Other controls follow
`y=1+2*x1+3*x2`, `y=3*2^x`, exact redundant columns, transposition, and
finite slopes (4,-3) for x=((1e308,1e308),(5e307,1e308)), y=(1e308,-1e308).
FORECAST.LINEAR agrees with TREND for the ordinary one-predictor system; its
separate legacy sample/coercion contract was deliberately preserved.

TREND retains its previously evidenced worksheet admission: intercept fits need
at least two observations and through-zero predictions need positive predictor
rank. Both its public modern helper and evaluator call the same fit method for
this check. Generic fitting and new coefficient functions remain rank-aware for
rank-zero designs. The independently recorded Calc controls in
`crates/poirot-test-runner/evidence/statistical-sample-requirements-libreoffice.json`
report VALUE for `TREND({3},{1},{3},TRUE)` and
`TREND({3;5},{0;0},{3},FALSE)`; those corpus expectations and their baseline
remain unchanged. Calc corroboration is not native Excel verification.
