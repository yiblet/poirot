# Modern inverse distributions and confidence margins

Contracts were read from the exact Microsoft function pages on 22 October 2026.
The following table records the documented numerical admission rules. All
parameters are scalar arguments that lift over compatible rectangular arrays;
none is a range reduction. Required omissions yield VALUE before numerical work.
Required operands are read left to right before mathematical fast paths.

| Function and source | Syntax and documented domain |
| --- | --- |
| [BETA.INV](https://support.microsoft.com/en-us/excel/functions/beta-inv-function) | `probability,alpha,beta,[A],[B]`; positive shapes; `0<p<=1`; omitted bounds independently default to 0 and 1. |
| [BINOM.INV](https://support.microsoft.com/en-us/excel/functions/binom-inv-function) | `trials,probability_s,alpha`; trials truncated, nonnegative; the current page excludes both endpoints of probability_s and alpha. Return the smallest integer whose cumulative mass is at least alpha. |
| [CHISQ.INV](https://support.microsoft.com/en-us/excel/functions/chisq-inv-function) | `probability,deg_freedom`; probability in [0,1]; degrees truncated to 1 through 10^10. |
| [CHISQ.INV.RT](https://support.microsoft.com/en-us/excel/functions/chisq-inv-rt-function) | Same syntax; probability in [0,1], truncated positive degrees; no documented upper degree bound; 64 unsuccessful search iterations yield NA. |
| [CONFIDENCE.NORM](https://support.microsoft.com/en-us/excel/functions/confidence-norm-function) | `alpha,standard_dev,size`; alpha strictly between 0 and 1, positive standard deviation; size truncated to at least one. Returns a margin. |
| [CONFIDENCE.T](https://support.microsoft.com/en-us/excel/functions/confidence-t-function) | Same syntax and positive alpha/deviation; size truncated; size one returns DIV0. Student degrees are size minus one. |
| [F.INV](https://support.microsoft.com/en-us/excel/functions/f-inv-function) | `probability,deg_freedom1,deg_freedom2`; probability in [0,1], both degrees truncated and positive. |
| [F.INV.RT](https://support.microsoft.com/en-us/excel/functions/f-inv-rt-function) | Same syntax; denominator degrees additionally must be less than 10^10. 64 unsuccessful search iterations yield NA. |
| [GAMMA.INV](https://support.microsoft.com/en-us/excel/functions/gamma-inv-function) | `probability,alpha,beta`; probability in [0,1], positive shape and scale; text arguments yield VALUE; 64 unsuccessful search iterations yield NA. |
| [LOGNORM.INV](https://support.microsoft.com/en-us/excel/functions/lognorm-inv-function) | `probability,mean,standard_dev`; strictly interior probability and positive standard deviation. |
| [T.INV](https://support.microsoft.com/en-us/excel/functions/t-inv-function) | `probability,deg_freedom`; `0<p<=1`, positive truncated degrees. Signed left quantile. |
| [T.INV.2T](https://support.microsoft.com/en-us/excel/functions/t-inv-2t-function) | Same input gates; nonnegative two-sided critical value. At p=1 it is zero. |

## Explicit compatibility choices and limits

A blank BETA bound is zero, whereas an omitted bound uses its independent default.
Equal/reversed bounds return NUM. Numeric text, logicals and blanks follow the
existing scalar projection for modern distributions, except GAMMA.INV rejects
text at each origin as its page explicitly requires. Unparseable text yields
VALUE; earlier operand errors propagate before later operands. The short pages
alone do not fully specify all native Excel origin/array/error precedence cases.
These chosen contracts are tested through Direct and Overlay; comprehensive
native coercion confirmation remains in the compatibility audit. Existing public
source/literal ingestion flushes supplied subnormals before these adapters. Pure
kernel tests cover raw binary64 subnormal operands; public tests cover normal
operands that create subnormal intermediate coordinates. Native Excel denormal
admission remains unverified.

An infinite mathematical quantile returns NUM rather than publishing infinity:
CHISQ.INV/GAMMA.INV/F.INV at p=1, right-tail chi-square/F at p=0 and T.INV at p=1.
Finite support endpoints publish zero or the beta upper bound. The pages admit
some infinite endpoints without explaining publication; these are explicit
binary64 publication choices pending native evidence. CONFIDENCE.T size below
one returns NUM, in addition to its documented size-one DIV0.

Shapes above 10^12, F/Student degrees above 2*10^12 and binomial trials above
10^12 require operational computational-limit failures, outside IFERROR. These
are implementation work envelopes, not invented Excel NUM domains. The narrower
per-name documented domains apply first. All nested tail/panel/refinement work
is metered before execution. Exhaustion of the 64-step inverse allowance is a
numerical failure: NA for the three names explicitly documenting that rule, NUM
for other names. Host work exhaustion remains operational.

## Equations and convergence

No external implementation, fixture, test or algorithm was copied or translated.
The kernels were derived from beta/gamma defining integrals, symmetry,
monotonicity and elementary change of variables. Those equations are documented
in [DLMF 8.17](https://dlmf.nist.gov/8.17) and
[DLMF 8.7](https://dlmf.nist.gov/8.7). `DISTRIBUTION_SOURCES.md` records the
already admitted bounded tail algorithms and their numerical limits.

Beta inversion uses the smaller supplied tail and reflected shapes. It retains
log endpoint distances throughout. Its derivative in log odds is
`x^a*(1-x)^b/B(a,b)`. Bounded expansion establishes a containing bracket;
safeguarded Newton steps refine it. The stop requires both tail-relative
residual and residual divided by this derivative, controlling log-odds error.
For small symmetric shapes near the center the algorithm integrates the density
from the symmetry center directly, retaining CDF minus one half rather than
subtracting two rounded probabilities. It reuses the existing bounded 8/16-point
panel helper, with every evaluation charged.

Gamma inversion solves in dimensionless scale-one coordinates. A Wilson-type
normal estimate is only a seed; it does not certify the answer. The defining
tail and density certify refinement. For a very small root, integration bounds
`exp(-x) <= exp(-t) <= 1` bound the error of the leading integral. The tiny-root
shortcut is admitted only when the leading root `x0` obeys `x0/a < 5*10^-16`. Then
`P(2*x0)>=p`, so the true root is at most `2*x0` and its log correction
is bounded by `2*x0/a < 10^-15`. Ordinary coordinates remain ordinary through final scaling;
zero/subnormal intermediate coordinates instead retain their logarithms.

Both inverse loops stop after 64 evaluations including bracketing. A containing
bracket with adjacent representable coordinates is an additional termination
certificate where no residual threshold is attainable. The selected value is an
evaluated final endpoint, chosen by its residual, never an older best estimate
outside the final bracket. Beta's coordinate is log odds: adjacent coordinates
bound the final odds by one log-coordinate ULP. This is a floating-point accuracy
certificate conditional on the admitted forward-tail accuracy, not a universal
mathematical interval proof for all shapes.

F maps beta log odds plus the degree ratio directly. Student t maps half the log
ratio, preserving small endpoint distances before projection. Confidence margins
retain a critical-value log until sigma/sqrt(size) has been applied when ordinary
intermediates would underflow or overflow. Beta interval interpolation recovers
scale before projecting zero/subnormal endpoint distances. Binomial inversion
uses an allocation-free integer binary search on [0,n], at most 41 search checks.
It chooses the smallest qualifying integer; numerical uncertainty in a rounded
CDF extremely close to an arbitrary alpha remains a forward-tail limit.

The existing MIT statrs 0.19.1 `erfc_inv` API is called directly for normal
quantiles. Source inspection verified fixed polynomial branches and no inverse
iteration loop. It retains twice the smaller probability rather than forming
2*p-1. No statrs inverse-CDF or inverse-beta iteration is called; pinned normal
and Student t implementations remain unchanged. No dependency was added.

## Independent controls and provenance

Elementary integrations give Beta(2,1) inverse sqrt(p), Beta(1,2) inverse
1-sqrt(1-p), Gamma(1,scale) inverse -scale*log1p(-p), F(2,2) inverse p/(1-p)
and Cauchy quartiles ±1. Binomial(4,1/2) cumulative numerators are 1,5,11,15,16
over 16. Tests include the next binary64 alpha above 5/16. Tiny half-shape gamma
follows erf(sqrt(x)) = 2*sqrt(x/pi)+O(x^(3/2)); these controls verify scaled
answers even when intermediate coordinates are zero or severely rounded
subnormals. They reject zero for representable tiny expected values.

Non-elementary controls are independently generated with mpmath 1.3.0, using
80/120-digit arithmetic and independent bisection on its defining-integral
APIs, plus tanh-sinh integration of unrearranged high-precision densities for
large-shape controls. The package is BSD 3-clause, verified from the installed
`mpmath-1.3.0.dist-info/LICENSE`; source/release:
https://github.com/mpmath/mpmath/tree/1.3.0 . It is an external calculator only;
no source or upstream test was copied or packaged. Calculation scripts, exact
binary64 inputs, precision-agreement checks and generated results are retained
with ticket evidence. Existing MPL FODS are run only as diagnostics and are not
sources for any new MIT tests or expected values.
