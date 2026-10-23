# Modern financial contracts

Reviewed the following Microsoft Excel pages on 22 October 2026. Each URL has the exact syntax and remarks used below. New implementations and expectations are independently authored from the stated equations; no third-party implementation, test or fixture was copied or adapted.

| Function | Primary page and implemented contract |
| --- | --- |
| CUMIPMT | https://support.microsoft.com/en-us/excel/functions/cumipmt-function — six required arguments, positive rate/periods/principal, inclusive one-based payment endpoints, timing exactly zero or one. |
| CUMPRINC | https://support.microsoft.com/en-us/excel/functions/cumprinc-function — same admission, principal rather than interest total. |
| DB | https://support.microsoft.com/en-us/excel/functions/db-function — rounded three-decimal declining rate, optional month twelve, first and last partial years. |
| DDB | https://support.microsoft.com/en-us/excel/functions/ddb-function — factor defaults two, capped remaining depreciable balance, no straight-line switch. |
| EFFECT | https://support.microsoft.com/en-us/excel/functions/effect-function — positive rate, truncated compounding count at least one. |
| FVSCHEDULE | https://support.microsoft.com/en-us/excel/functions/fvschedule-function — multiply principal by each one-plus-rate; only numeric or blank schedule cells, blanks zero; no ignored text/booleans or skipped trailing errors. |
| ISPMT | https://support.microsoft.com/en-us/excel/functions/ispmt-function — even-principal zero-based interest equation. |
| NOMINAL | https://support.microsoft.com/en-us/excel/functions/nominal-function — positive effective rate, truncated compounding count at least one. |
| PDURATION | https://support.microsoft.com/en-us/excel/functions/pduration-function — all arguments positive, logarithmic growth duration. |
| RRI | https://support.microsoft.com/en-us/excel/functions/rri-function — positive period count and real positive-base growth equation. |
| SLN | https://support.microsoft.com/en-us/excel/functions/sln-function — cost minus salvage divided by life. |
| SYD | https://support.microsoft.com/en-us/excel/functions/syd-function — sum-of-years-digits depreciation. |
| VDB | https://support.microsoft.com/en-us/excel/functions/vdb-function — fractional interval, factor two, no_switch false by default; choose larger straight-line allowance at each period. |
| XIRR | https://support.microsoft.com/en-us/excel/functions/xirr-function — original first date anchor, 365-day years, integer-truncated dates, both flow signs, guess .1, at most 100 Newton iterations, failure NUM. |
| XNPV | https://support.microsoft.com/en-us/excel/functions/xnpv-function — positional flow/date pairs, original first date anchor, 365-day years, integer dates, mismatched counts or earlier dates NUM, invalid dates VALUE. |

The related Microsoft explanation https://support.microsoft.com/en-us/excel/go-with-the-cash-flow-calculate-npv-and-irr-in-excel establishes that multiple roots are possible and changing the guess may select another root. No nearest-root claim is made.

## Choices that need native confirmation

Native Excel is unavailable. The following are explicit implementation choices or unresolved contracts, retained for audits 024/025 rather than represented as measured compatibility:

* Scalar numeric parameters use existing Poirot numeric coercion, including numeric/date text and booleans. Omissions default only optional parameters; actual blank cells are numeric zero. Required syntactically missing scalar operands are VALUE. Detailed origin/error precedence and computed-array behavior require native probes.
* Scalar parameter functions broadcast rectangular arrays. FVSCHEDULE lifts principal only, XNPV rate only, XIRR guess only; schedule and paired collections never determine output geometry. XIRR with omitted guess is a scalar default and makes no out-of-range read. Single-member output is scalar. Exact native array contexts remain unverified.
* Cumulative endpoints are validated before truncation, truncated to integer payments, and cannot exceed nper. Microsoft does not settle fractional endpoints or beyond-term acceptance. Positive fractional nper remains meaningful in the annuity equation. Timing must be exactly zero or one.
* DB truncates period and month and permits one final partial period when month differs from twelve. Fractional life and zero cost/salvage policies are not fully specified. Cost at/below salvage returns zero. DDB accepts positive fractional periods using its geometric equation; fractional-period native semantics need confirmation.
* ISPMT uses the documented zero-based equation without an extra period-range rejection, resolving its contradictory argument table/remarks in favor of the latter. Zero nper gives DIV0. SLN allows signed arguments and gives DIV0 for zero life; SYD requires positive life and period within life. These boundary spellings need native confirmation.
* RRI accepts matching negative present/future signs via their positive ratio and a zero future value as -1; rejects zero present, opposite signs and nonpositive nper. This follows the real positive-base equation, not an independently observed Excel domain table.
* Schedule errors remain errors, nonnumeric members are VALUE. Paired cash-flow blanks mean a zero flow at the same date; text/logicals are VALUE and are never dropped. Dates require numeric nonnegative finite serials validated through the workbook epoch calendar, then integer-truncated. Text and blank dates are VALUE. Equal dates and unordered dates after the anchor are allowed. Orientation is flattened row-major independently while positions remain aligned. The public pages do not settle all these policies or multisheet ordering.
* XNPV admits single-sign flows because its equation is defined; the page's descriptive sign wording needs native verification. XIRR rejects same-sign and all-identical-date degenerate schedules. It applies ordinary Newton steps in rate coordinates from the supplied guess, with a scaled residual and small-step certificate; no bracket/grid fallback changes the basin. A nonfinite/<=-1 step, zero derivative with nonzero residual, numerical stagnation or 100 tries produces NUM. An exactly satisfied normalized equation can return even with zero derivative, including a double root at zero. Native multiple-root basin details remain unknown.
* VDB currently has an explicit operational envelope of at most 100,000 preceding/requested periods. Each visited period is charged. A huge life alone does not reject a short early interval. Late intervals beyond the envelope abort operationally, outside IFERROR; this is a computational limitation, not an Excel NUM domain.

## Independent evidence

A two-period loan at .1 on 210 has end payments121, principal100/110, interest21/11; beginning payments110, principal110/100, interest0/10. The kernels use geometric interval formulas with logarithmic small-rate corrections; no amortization vector or period walk. Extreme beginning payment on principal2 at rate1e308 is representable and remains2.

Depreciation expectations use straight-line arithmetic, geometric balances and separately computed fractional overlaps. An asset costing 1000 with zero salvage, life 5 and factor 2 gives declining allowances400,240,144; the straight-line tail is108,108. The interval 2.5–4.5 therefore totals 72 + 108 + 54 = 234, versus 72 + 86.4 + 25.92 = 184.32 without switching.

Yearly flows [-100, 230, -132] factor to roots .1/.2; independent guesses exercise both. [-100, 200, -100] has a double root at zero. [-100, 50, -100] has negative discriminant and no positive-base root. Scaling of intermediate discount terms and compensated summation are numerical methods independently applied to the defining exponential equation.

The cumulative product is normalized into binary mantissas and exponents before multiplying principal, payment coefficient, interval length and the interest ratio. Independently calculated 240-digit controls cover the largest finite principal and a subnormal principal over 1e20 periods at rate 1e-20: their total interests are approximately -1.0462155305887664e308 and -5e-324. The logarithm-of-logarithm correction uses six formal-series coefficients below rate 1e-3; independent Decimal cases straddle the regime boundary. These are numerical checks, not native Excel measurements.

Period offsets are applied after subtracting the supplied end and total period count, so a one-period beginning-payment adjustment cannot disappear when both counts are very large. Discounted equations use Neumaier compensated addition; zero-rate values retain a finite linear reconstruction scale to preserve cancellation such as -1e308 + 1 + 1e308 = 1.
