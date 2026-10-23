# Exponential smoothing contracts and numerical policy

The four FORECAST.ETS functions share one independently written additive-error,
additive-trend, additive-seasonal innovations model. This is a working fitted
forecast, not a claim to reproduce Excel's undocumented optimizer.

## Primary worksheet evidence

Read on October 22, 2026:

- [Microsoft FORECAST.ETS VBA](https://learn.microsoft.com/en-us/office/vba/api/excel.worksheetfunction.forecast_ets)
  describes AAA, implicit sorting, monthly/yearly/numeric timelines, gap completion,
  explicit periods through 8,760, and future targets.
- [Microsoft FORECAST.ETS.STAT](https://support.microsoft.com/en-us/excel/functions/forecast-ets-stat-function)
  specifies selectors 1 through 8 as alpha, beta, gamma, MASE, SMAPE, MAE, RMSE,
  and step. Its MAE explanation accidentally repeats the SMAPE explanation.
- [Microsoft FORECAST.ETS.SEASONALITY](https://support.microsoft.com/en-us/excel/functions/forecast-ets-seasonality-function)
  requires automatic periods to agree with the forecast function.
- [Microsoft XlForecastAggregation enum](https://github.com/MicrosoftDocs/VBA-Docs/blob/76d3237f203cb07588265904c83d22186b670e81/api/Excel.xlforecastaggregation.md)
  explicitly maps Average=1, Count=2, CountA=3, Max=4, Median=5, Min=6, Sum=7.
  The inspected content SHA256 is
  `0886a8b3ce98375625e3985c04c01b47598bdd699577681581824febe8743354`.

These primary documents conflict. Older parameter prose calls default aggregation
0 and lists other numbers in another order; the explicit named enum is used here.
Omission chooses Average; explicit 0 is an invalid enum value. Duplicate timestamps
are aggregated despite the same pages also saying duplicates produce an error.
Missing points may occupy **up to and including 30 percent** of the completed grid,
following the inclusive prose. These are explicit policies pending native Excel
observations, not claims that the contradictions were resolved experimentally.

## Preparation and argument policy

Collections must have identical two-dimensional geometry and one sheet. Each
position must contain an actual finite number on both axes; errors propagate and
nonnumeric/blank positions return VALUE rather than silently breaking alignment.
COUNT and COUNTA therefore agree for the currently admitted numeric input domain.
Broader text/blank admission, origin distinctions, and their interaction with
COUNT/COUNTA need native evidence. Equal counts in different shapes return NA.

Sort timestamps, aggregate duplicates, then infer the smallest observed positive
step whose integral multiples contain every observation. Missing consecutive
points receive weighted linear interpolation between the surrounding observations,
or zero with completion=0. For timestamps representing the same calendar day at
least 28 days apart, use calendar-month indices in the workbook's 1900/1904 epoch;
this covers first-of-month and yearly grids without pretending every month has
30 days. STAT step is measured in months for this calendar grid. Numeric grids
report their numeric step. The phantom 1900 leap day is not a calendar-month date.
The smallest observed gap cannot reveal a finer unobserved grid; that ambiguity
also needs native evidence. Targets before the final timestamp are NUM. Fractional
numeric horizons linearly interpolate adjacent seasonal projections; fractional
calendar dates with another day are currently NUM. Horizons above one billion
steps are outside the current declared numerical projection envelope and report
an operational unsupported-capability failure, not a catchable NUM error.

Selectors are exact integers, not silently truncated. Missing optional scalar
arguments use their defaults; actual blanks numerically coerce to zero and can
therefore be invalid selector values. Scalar targets/options/selectors lift over
arrays; values/timeline collections do not drive output dimensions. Invalid arity
is rejected before reading arguments. Pinned POI registration remains unchanged.

## Model and bounded fitting policy

Normalize observations by their maximum absolute value. For period m>0, initialize
trend from the difference of the first two cycle means divided by m. Initialize
the first cycle's level and zero-mean seasonal phases by removing that trend.
For m=0, initialize from the first observation and first difference. State begins
immediately before the first observation. Seasonal fits require two complete
cycles; this initialization/minimum is an implementation choice.

For prediction p=level+trend+old_season, innovation e=y-p, update:

```
level  <- level + trend + alpha * e
trend  <- trend + beta * e
season <- old_season + gamma * e
```

Optimize actual one-step SSE over bounded external weights a,b,g in [0,1], mapping
alpha=a, beta=a*b, gamma=(1-a)*g. Thus STAT beta and gamma are direct innovations
gains, not external Holt weights. Nonseasonal gamma is zero. Four declared starts
(0.1,0.1,0.1), (0.5,0.5,0.5), (0.9,0.1,0.9), and (0,0,0) each use coordinate
search with initial distance 0.25, halving when no improvement occurs. Stop at an
exact zero SSE, distance below 1e-5, or three successive complete sweeps with
relative SSE improvement no larger than 1e-8, including a positive improvement.
A no-improvement sweep can extend this objective-convergence sequence but cannot
start it: an initially coarse plateau must still refine its step. There are at
most 1,024 sweeps per start. Select the best converged fit; no converged fit is an operational unsupported-capability failure.
Relative SSE convergence has no absolute floor that would erase residuals on
large-offset data. Translation/scaling controls exercise this boundary.
This is deterministic bounded local fitting, without a global-optimum guarantee.
Initial states are fixed by the declared initialization, not jointly optimized.

Automatic detection removes a least-squares straight trend solely for period
scoring. For each period 2 through min(n/2,8760), sum squared phase-mean residuals
and add detrended_SSE*period/n. Require the penalized score below 80 percent of
nonseasonal detrended SSE and prefer the first/smallest period on exact ties.
Numerically negligible trend residuals choose nonseasonal. This is an explicit
model-selection policy, not observed Excel seasonality detection. The selected
period then uses the exact same fitted innovations model as explicit periods.

All one-step residuals, including initialization observations, enter SSE and
metrics. MAE=mean(abs(e)); RMSE=sqrt(SSE/n); SMAPE=mean(2*abs(e)/(abs(y)+abs(p)))
with a zero/zero summand defined as zero. MASE divides MAE by mean absolute lag-m
changes (lag 1 without seasonality). Zero denominator returns zero only for an
exact fit; otherwise DIV0. These denominators, warm-up policy and gain reporting
are declared mathematics; Excel's corresponding conventions are not established.

Confidence is a Gaussian future-observation radius, using innovation variance
SSE/n and the exact undamped additive innovations impulse response. With integer
h>=1 and k=floor((h-1)/m), the variance multiplier is:

```
1+(h-1)*alpha^2+alpha*beta*h*(h-1)
 +beta^2*h*(h-1)*(2*h-1)/6
 +gamma*k*(2*alpha+gamma+beta*m*(k+1))
```

Nonseasonal k=0. Fractional horizons conservatively use ceil(h) for the radius.
Use the existing stable normal confidence kernel for coverage >=0.5; smaller
coverage passes the coverage itself to statrs erf_inv, avoiding 1-coverage
cancellation. No new statistical dependency or copied implementation is added.
A perfect fitted model has zero estimated variance and a zero radius. Native
Excel confidence normalization and fractional-date handling remain unverified.

## Independent controls and resource boundaries

The fixed-state control starts at level=10, trend=1, phases=(-2,2), gains
(1/2,1/4,1/8), observations=(11,14). Direct rational substitution gives predictions
9 and 31/2, innovations 2 and -3/2, final level 51/4, trend 9/8, phases
(-7/4,29/16), and future forecasts 97/8 and 269/16. SSE=25/4 and RMSE=sqrt(25/8).
The future variance multipliers at horizons 1,2,3 are 1,25/16,181/64. These are
independent equations, not results harvested from Poirot. Perfect lines and
repeated patterns independently establish fitted forecasts; noisy data verifies
the optimizer improves its objective instead of merely returning fixed weights.

Reserve fallibly and check the expanded grid before allocation. The adapter
admits a sound upper bound for simultaneous sample/state storage, charges sorting
and completion, and charges every detection/objective pass before executing it, plus one work
unit for each optimizer sweep.
Detection is bounded by 8,760 candidate periods; each objective is O(n) with O(m)
seasonal storage. No work failure is a catchable worksheet error. STAT step skips
fitting; forecasts do not construct confidence paths or prediction tables. No
cache, thread, VM, source schema or external runtime is introduced.

Apache POI 5.5.1 has no ETS implementation and augurs-ets 0.10.2 lacks seasonal
models. No code, tests, datasets or algorithms were copied or adapted from them,
statsmodels, anofox, R, or the segregated MPL diagnostic fixtures. The innovations
recursion and variance controls above were independently derived. Native optimizer,
initialization, automatic period selection and metric parity remain compatibility
work; registration does not hide these limits.
