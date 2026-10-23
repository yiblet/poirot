# Security function contracts

Reviewed 22 October 2026. Implemented independently from Microsoft worksheet equations and elementary calendar/discount mathematics. No external implementation, test fixture, security data or dependency was introduced. Existing common Gregorian calendar conversion and the financial crate’s binary-scaled product retain their existing provenance.

## Primary references

* ACCRINT: https://support.microsoft.com/en-us/excel/functions/accrint-function
* ACCRINTM: https://support.microsoft.com/en-us/excel/functions/accrintm-function
* COUPDAYBS: https://support.microsoft.com/en-us/excel/functions/coupdaybs-function
* COUPDAYS: https://support.microsoft.com/en-us/excel/functions/coupdays-function
* COUPDAYSNC: https://support.microsoft.com/en-us/excel/functions/coupdaysnc-function
* COUPNCD: https://support.microsoft.com/en-us/excel/functions/coupncd-function
* COUPNUM: https://support.microsoft.com/en-us/excel/functions/coupnum-function
* COUPPCD: https://support.microsoft.com/en-us/excel/functions/couppcd-function
* DISC: https://support.microsoft.com/en-us/excel/functions/disc-function
* DURATION: https://support.microsoft.com/en-us/excel/functions/duration-function
* INTRATE: https://support.microsoft.com/en-us/excel/functions/intrate-function
* MDURATION: https://support.microsoft.com/en-us/excel/functions/mduration-function
* PRICE: https://support.microsoft.com/en-us/excel/functions/price-function
* PRICEDISC: https://support.microsoft.com/en-us/excel/functions/pricedisc-function
* PRICEMAT: https://support.microsoft.com/en-us/excel/functions/pricemat-function
* RECEIVED: https://support.microsoft.com/en-us/excel/functions/received-function
* TBILLEQ: https://support.microsoft.com/en-us/excel/functions/tbilleq-function
* TBILLPRICE: https://support.microsoft.com/en-us/excel/functions/tbillprice-function
* TBILLYIELD: https://support.microsoft.com/en-us/excel/functions/tbillyield-function
* YIELD: https://support.microsoft.com/en-us/excel/functions/yield-function
* YIELDDISC: https://support.microsoft.com/en-us/excel/functions/yielddisc-function
* YIELDMAT: https://support.microsoft.com/en-us/excel/functions/yieldmat-function

Modern day-count implementation notes: https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/220caf3e-aaf3-483f-844a-454abcf07bd2 . ACCRINT calc_method and basis notes: https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/e32cd66e-a035-4338-9e6e-a75929aaa73f . The current European convention changes 31 to30 and leaves February unchanged; the older MS-OE376 note differs. These are contract facts, not copied algorithms.

## Admission and defaults

All scalar parameters lift through existing rectangular broadcasting. Dates use the date-family context/coercion boundary; invalid dates map to VALUE while operand spreadsheet errors and operational failures survive. Required omissions are VALUE. Actual blank cells coerce to zero. Numeric parameters use existing numeric coercion, including Boolean/numeric text admission; detailed origin/error precedence and native array contexts remain unverified.

Dates, basis and frequency are integer-truncated. Basis defaults to NASD0 and admits0..4; frequency admits1/2/4. COUP* arity3..4; DURATION/MDURATION5..6; PRICE/YIELD6..7; DISC/INTRATE/PRICEDISC/RECEIVED/YIELDDISC4..5; PRICEMAT/YIELDMAT5..6; Treasury exactly3; ACCRINT6..8; ACCRINTM3..5. ACCRINT and ACCRINTM omitted par use1000; actual blank par is zero and NUM. ACCRINT basis defaults0 and calc_method TRUE. The public page calls par required while specifying an omission default: this implementation admits trailing omitted ACCRINTM par and an explicit ACCRINT missing par slot.

Settlement must precede maturity for COUP*, regular bonds and discount securities. Coupon/yield inputs of PRICE/DURATION/MDURATION and maturity-interest functions admit zero but reject negatives; redemption is positive. YIELD requires positive price and may return a negative real root, with per-period base positive for multi-coupon bonds; native negative-root behavior remains unverified. ACCRINT/M require positive rate/par and issue before settlement. ACCRINT additionally requires first_interest after issue; calc_method FALSE starts at first_interest only when settlement is later. The latter date rejection is an explicit choice needing native confirmation. Maturity-interest functions currently require issue no later than settlement, another incompletely documented boundary.

Discount equations require positive amounts/rate. Discount uses redemption as denominator; INTRATE/YIELDDISC use investment/price. A zero computed day span gives DIV0. RECEIVED requires positive remaining discount denominator (NUM otherwise). Native error classification at vanishing/negative denominators remains to be verified.

Treasury functions admit equal settlement/maturity for TBILLPRICE/TBILLEQ, as those pages specify greater-than rejection; TBILLYIELD rejects equality. The maturity limit is a calendar anniversary, retaining the original month/day (clamped leap day). Actual elapsed serial days are used. TBILLEQ uses the documented365*discount/(360-discount*days) even for longer bills; the page’s “360 basis” phrasing does not establish NASD day adjustment or a separate long-bill equation. Native long/equal/leap/negative-denominator behavior remains unverified.

## Calendar and equations

Worksheet serial60 remains the fictitious1900-02-29 and actual elapsed days count it. Excel1904 serials convert with offset1462; supplied serials are bounded through9999-12-31. Coupon boundaries can precede the admitted input epoch; returned coupon serials preserve that signed numeric boundary. Maturity-anchored month arithmetic retains end-of-month intent and never repeatedly clamps an already shifted date.

Coupon denominators are360/frequency for bases0/2/4,365/frequency for3, actual neighboring-coupon difference for1. Numerators are independently counted; accrued plus remaining is not forced to equal the denominator. General Actual/Actual year lengths follow the modern implementation note’s leap/average-year rule; coupon normalization instead uses its own period. Native per-function annual-basis nuances, leap-slot treatment and conflicting February notes require probes.

Regular pricing distinguishes the final simple-interest coupon from multi-coupon exponential discounting. Macaulay duration uses periodic discounted cash-flow weights, including one coupon; modified duration divides by1+yield/frequency. Geometric and weighted geometric blocks concatenate in logarithmic work without payment/date vectors. Zero-coupon duration is the payment time even when its discounted price underflows. Monetary weights are normalized independently from representable money. Clean coupon coefficients combine accrual before multiplying large money.

ACCRINT basis1 computes fractional positions in first-interest-anchored quasi periods; complete periods count arithmetically. Additive European/Actual360/Actual365 bases use their directed whole-interval numerator over the fixed coupon denominator. NASD instead seeks the first quasi boundary and sums each adjusted fragment separately; complete adjusted fragments are never forced to one period. Integral fragment days sum exactly and divide by360 once. Each visited fragment is charged; an explicit100,000-fragment envelope aborts operationally. The admitted worksheet calendar has fewer periods than this envelope. Native ACCRINT normal-period denominators and February normalization remain an explicit uncertainty, rather than a claimed native verification.

YIELD normalizes cash amounts, establishes an upper bracket by bounded expansion, then uses Newton with midpoint safeguards. At most100 charged passes cover both expansion and refinement. Every returned iterative solution has a scaled price residual and rate-step certificate (or an exactly satisfied represented equation). No clamped/stagnant guess is returned as success. This is a bounded specialized monotone bond solver; it does not reuse the periodic RATE/IRR solver or change root search globally.

Independent tests derive regular bond price100 from10/1.1+110/1.1², duration21/11 and modified210/121; a two-year zero-coupon price4 implies yield4. Half-year simple discount gives price95, discount .1 and investment yield2/19. A 90/360-year accrual on1000 at .1 is25. These mathematical controls and independently authored epoch/schedule boundaries are distinct from native Excel observations. No native Excel execution was available.

The ACCRINT implementation note states a par default0.001, conflicting with the worksheet support pages’1000. This implementation follows the public worksheet default1000; native confirmation remains required. Shared scaled quotient arithmetic retains binary exponents for both numerator and denominator factors, avoiding overflowing reciprocals or transient quotients. No additional algorithm implementation was imported.

Discount coefficients remain logarithmic until combined with monetary factors. When a coefficient is subnormal or overflows by itself, logarithms combine before reconstructing money; a normal coefficient keeps binary-scaled product arithmetic. This preserves a two-year zero-coupon redemption1e308 discounted at yield1e200 as price approximately1e-92, rather than zero.
