# Irregular coupon and French depreciation contracts

These kernels and tests were independently written from the equations and contract facts below. No external implementation, test, fixture or dataset was copied, translated or adapted. Existing Poirot calendar, scaling, geometric-sum and decimal rounding helpers are reused. The segregated LibreOffice corpus may be executed as a diagnostic; it is not source material for these additions.

Primary Microsoft worksheet pages, inspected 22 October 2026:

- https://support.microsoft.com/en-us/excel/functions/oddfprice-function
- https://support.microsoft.com/en-us/excel/functions/oddfyield-function
- https://support.microsoft.com/en-gb/excel/functions/oddlprice-function
- https://support.microsoft.com/en-us/excel/functions/oddlyield-function
- https://support.microsoft.com/en-us/excel/functions/amorlinc-function
- https://support.microsoft.com/en-us/excel/functions/amordegrc-function
- https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/1fc49c43-5eea-4428-81de-d49f87d02c8a
- https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/d2ef143a-f340-4191-814c-23a091dd6f1d
- https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/3bed0837-e3f9-4f4e-bbc9-19122d2c9545
- https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oe376/e403914c-0615-4c35-9fbb-c2931ac3b7cb

## Admitted contracts

ODDFPRICE and ODDFYIELD require eight arguments; ODDLPRICE and ODDLYIELD require seven. Redemption is required and strictly positive. Only basis is optional, default zero. Bases are integer-truncated 0..4; frequencies use the existing security admission of truncated 1, 2 or 4. First-period dates must satisfy issue < settlement < first coupon < maturity. Last-period dates must satisfy last interest < settlement < maturity. Invalid dates produce VALUE, negative coupon/yield and nonpositive price produce NUM. Dates are truncated while retaining workbook epoch and date-context reads.

The first coupon must lie on the maturity-anchored regular schedule. Quasi-boundaries retain that original anchor, including a first coupon clamped in February. The first cash flow is the regular coupon times the sum of issue-to-first quasi fractions; accrued interest uses separate directed issue-to-settlement fragments. Its discount time is the count of whole settlement-to-first quasi periods plus the final DSC/E fraction. Later regular coupons and redemption are counted and summed geometrically in logarithmic work. The short and long worksheet equations then share one cash-flow representation. No coupon date or cash-flow vector is allocated.

Last-period price uses separate total, accrued and remaining quasi fractions. Its equation is (redemption + coupon*total)/(1 + yield/frequency*remaining) - coupon*accrued. ODDLYIELD is its direct algebraic inverse. ODDFYIELD uses a bounded safeguarded Newton solve with at most 100 charged passes, finite upper-bracket establishment, a scaled price residual and a rate-step convergence check. No last iterate is returned as a false success. Negative inverse yields above the periodic singularity remain possible.

Actual/Actual quasi fractions use arithmetic containing-period positions with each period's actual denominator. Actual/360 and Actual/365 use additive directed elapsed days and their independent annual denominators. NASD and European directed fragments are not assumed additive across quasi boundaries: each is charged and summed without a schedule allocation, with a 100,000-fragment operational ceiling. Operational exhaustion escapes IFERROR. Calendar and regular coupon conventions remain those documented in SECURITY_SOURCES.md.

Both AMOR functions require six arguments and accept optional basis 0, 1, 3 or 4; basis 2 is rejected. Period and basis use integer truncation. Cost and salvage must be nonnegative, salvage cannot exceed cost, rate must be positive, and purchase cannot follow the first accounting date. The depreciation consumer applies the Office note's symmetric February28/29→30 and day31→30 adjustments for bases0/4. Actual uses366 if the interval contains a leap day, otherwise365; basis3 uses365. Gregorian century gaps and the worksheet leap slot are explicit.

AMORLINC numbers the initial prorated charge as period0, then selects fixed annual charges and the last remaining depreciable amount directly. Charges are capped before overflowing an uncapped monetary product. AMORDEGRC chooses coefficient1.5 for life in [3,4],2 for [5,6],2.5 above6, and NUM in the other intervals. Its implemented schedule numbers the uncapped rounded prorated charge as period0. Ordinary rounded accelerated declining charges continue until the next charge would exceed the remaining cost minus salvage; that transition emits a rounded half-book terminal charge, and subsequent periods are zero under the cumulative stopping condition. Initial and terminal charges can exceed the remaining depreciable amount; AMORLINC alone applies the salvage cap before returning each charge. This replaces a rejected interpretation that scheduled terminal rates at fixed ceil(life) indices. Whole-unit rounding uses the existing spreadsheet decimal HalfUp helper at every step. At most100,000 requested preceding periods are allowed and each is charged; a stationary rounded-zero balance can return zero directly. No geometric shortcut replaces intermediate rounding.

## Compatibility limits requiring native worksheet evidence

Native Excel was unavailable. Registration means that all six computational functions have actual kernels and adapters; it does not certify every undocumented worksheet corner. Audit024/025 must retain these questions:

- Native error precedence, date text, logical/numeric-text and reference-origin admission; actual blank versus omitted or missing required arguments; fractional frequency and period admission; array broadcast semantics and old CSE contexts. Current adapters share established date-aware security coercion and rectangular lifting.
- NASD and European February differences between current worksheet behavior and differing Office notes; quasi-period denominator choices for bases2/3; native handling of ambiguous month-end intent and a first coupon not on the maturity schedule.
- ODDFYIELD's native initial guess, negative-root basin, extreme-scale convergence tolerance and the exact 100-pass failure behavior. Mathematical monotonicity justifies the bounded safeguards, but they are not a captured native execution trace.
- AMORDEGRC's coefficient endpoint inclusivity, whole-unit tie rule, detailed salvage behavior and precisely how the published final50%/100% language maps to the terminal transition. Microsoft publishes coefficients and the terminal rates but no complete worksheet recurrence. The explicit implementation above is a documented interpretation, not an observation of native Excel. DAX's nearest-integer admission is deliberately not used as worksheet evidence.
- AMOR first-period equality, periods beyond useful life, dates spanning multiple years, cost/rate zero behavior and very large periods. Authored mathematical tests specify today's implementation without disguising those unknowns as native-verified expectations.

No source schema or artifact envelope changes are needed. Callable registration and shape/effect metadata advance the builtin and portable plan contracts together with both retained-overlay compatibility boundaries.

## Independent observable comparison

A bounded diagnostic executed our own formulas through the existing LibreOffice26.8.1.1 public adapter (isolated image sha256:a92195f0b290d1302b0e27efbb9ac07b9982ddceaa09e15bacddfd21f588f220). Successive-period observations reject the initial fixed-life terminal interpretation: cost1000/salvage100/rate.2 with a quarter-year initial period yields100,360,216,130,78,58,0. These observations support the cumulative crossing interpretation; they are not native Excel proof. The arithmetic regression expectations are independently derived from each declining balance and rounded charge, with the chosen terminal policy stated above. No LibreOffice source, implementation algorithm, fixture or test was accessed or reused. The comparison wrapper, original inputs, raw response and rejected-policy differences remain outside the repository in ticket evidence. Native confirmation and the residual100% terminal-language question remain audit025 work.

A second original boundary diagnostic observed100,450,0 for cost1000, rate.2, quarter-year initial period and salvage999,1000 or899.6. It establishes that Calc does not cap the initial charge or stop immediately after an initial crossing. The selected declining policy keeps that distinction explicit. Apache POI's positive AMORDEGRC probe returned NotImplementedFunctionException, so it contributes no numerical evidence. Exact image IDs and raw diagnostics remain in ticket evidence; no timing comparison is claimed.
