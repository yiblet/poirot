# Modern probability functions

These 18 functions use the Microsoft worksheet contracts and independently
implemented mathematical equations. No implementation, tests or fixtures were
copied or translated from statrs, Apache Commons, LibreOffice or another project.
The existing pinned numerical paths remain separate.

## Worksheet contracts

Microsoft Support pages were consulted on 22 October 2026. Each link below names
the corresponding function, rather than a secondary implementation. Numeric
parameters are finite scalar numbers after the evaluator's scalar coercion;
invalid numeric domains return `#NUM!`, failed text conversion returns `#VALUE!`,
and existing cell errors propagate. A required omitted parameter returns
`#VALUE!`. Actual blank cells coerce to zero. Numeric text and logical values use
the existing scalar numeric policy for direct, array and reference origins.
Cumulative selectors use the existing logical coercion policy, including numeric
zero/nonzero and logical text. The native origin-sensitive details, blank versus
omitted selectors, and error precedence are not established by these pages;
they remain explicit compatibility audit work, rather than native-test claims.

All parameters are scalar parameters that broadcast over rectangular arrays.
There are no collection reductions in this family. The host owns spill discovery,
collision handling, lazy branches, dependencies and execution budgets.

| Functions and primary source | Signature and mathematical domain |
| --- | --- |
| [BETA.DIST](https://support.microsoft.com/en-us/excel/functions/beta-dist-function) | `x, alpha, beta, cumulative, [A], [B]`; shapes positive, `A <= x <= B`, `A < B`. Omitted A and B independently default to 0 and 1. A blank bound is zero. Density includes the interval Jacobian; the CDF does not. |
| [BINOM.DIST](https://support.microsoft.com/en-us/excel/functions/binom-dist-function) | `successes, trials, probability, cumulative`; counts truncate, `0 <= successes <= trials`, probability in [0,1]. Zero trials and degenerate probabilities are supported. |
| [BINOM.DIST.RANGE](https://support.microsoft.com/en-us/excel/functions/binom-dist-range-function) | `trials, probability, low, [high]`; inclusive truncated count interval within [0,trials]. Omitted high defaults to low; blank high is zero. Probability remains real despite the page's broad integer wording. |
| [CHISQ.DIST](https://support.microsoft.com/en-us/excel/functions/chisq-dist-function), [CHISQ.DIST.RT](https://support.microsoft.com/en-us/excel/functions/chisq-dist-rt-function) | `x, degrees, cumulative` and `x, degrees`; degrees truncate into 1 through 10^10. Left cumulative, density or direct right tail; x must be nonnegative. The RT page does not state its negative-x result, so the chosen `#NUM!` boundary still needs native confirmation. |
| [EXPON.DIST](https://support.microsoft.com/en-us/excel/functions/expon-dist-function) | `x, rate, cumulative`; x nonnegative and rate positive. |
| [F.DIST](https://support.microsoft.com/en-us/excel/functions/f-dist-function), [F.DIST.RT](https://support.microsoft.com/en-us/excel/functions/f-dist-rt-function) | `x, degrees1, degrees2, cumulative` and `x, degrees1, degrees2`; x nonnegative, each truncated degree at least one. The right tail is evaluated directly, without subtracting a rounded CDF. |
| [GAMMA](https://support.microsoft.com/en-us/excel/functions/gamma-function) | One real argument, excluding zero and negative integer poles. Negative nonintegral values retain their sign. |
| [GAMMA.DIST](https://support.microsoft.com/en-us/excel/functions/gamma-dist-function) | `x, alpha, beta, cumulative`; x nonnegative, both parameters positive. Beta is a scale, not a rate. |
| [GAMMALN](https://support.microsoft.com/en-us/excel/functions/gammaln-function), [GAMMALN.PRECISE](https://support.microsoft.com/en-us/excel/functions/gammaln-precise-function) | One positive argument. Both compute log gamma directly, without first forming an overflowing gamma value. |
| [GAUSS](https://support.microsoft.com/en-us/excel/functions/gauss-function), [PHI](https://support.microsoft.com/en-us/excel/functions/phi-function) | One real argument. GAUSS is the signed standard normal integral from zero; PHI is the standard normal density. |
| [HYPGEOM.DIST](https://support.microsoft.com/en-us/excel/functions/hypgeom-dist-function) | `sample successes, sample size, population successes, population size, cumulative`; the four counts truncate. Sample size, population successes and population size are positive; sample and success counts cannot exceed population. Sample successes must lie in the feasible support, even for a CDF. The logical selector is not an integer count. |
| [LOGNORM.DIST](https://support.microsoft.com/en-us/excel/functions/lognorm-dist-function) | `x, mean, standard deviation, cumulative`; x and standard deviation positive. |
| [NEGBINOM.DIST](https://support.microsoft.com/en-us/excel/functions/negbinom-dist-function) | `failures, successes, probability, cumulative`; counts truncate, failures nonnegative, successes at least one, probability in [0,1]. Counts failures before the requested success. |
| [WEIBULL.DIST](https://support.microsoft.com/en-gb/excel/functions/weibull-dist-function) | `x, shape, scale, cumulative`; x nonnegative, shape and scale positive. |

Singular mathematical endpoint densities are published as `#NUM!`; finite limits
are evaluated explicitly. Native endpoint/error precedence and Excel's handling
of subnormal inputs and outputs still require independent native evidence. The
extreme-scale tests establish this kernel's binary64 policy, not Excel's numeric
admission policy. Registered support does not claim bit-for-bit native equality.

## Numerical equations and bounds

The mathematical sources are NIST DLMF [gamma definitions](https://dlmf.nist.gov/8.2),
[gamma series](https://dlmf.nist.gov/8.7),
[gamma fractions](https://dlmf.nist.gov/8.9),
[beta integrals](https://dlmf.nist.gov/8.17), and
[Stirling expansion and remainder](https://dlmf.nist.gov/5.11).
These are equations and mathematical facts; no source implementation was reused.
The Stirling correction uses the rational Bernoulli coefficients through a^-15.
For positive a >= 8 the next alternating term bounds the omitted remainder.
The small `logGamma(1+a)` polynomial comes from the convergent zeta series in
[DLMF 5.7.3](https://dlmf.nist.gov/5.7.E3); below 0.001 its omitted term is less
than a^7/(7*(1-a)), before floating-point rounding.

For a tiny beta shape whose lower probability is almost one, a direct positive
upper-tail integral uses t=exp(-s). Since a<1, its defining integral also
gives Q(x) <= x^(a-1)*(1-x)^b/(b*B(a,b)). A log bound below -800
proves underflow before sampling an unnecessarily narrow endpoint layer. For the other shape below one, u=s^b
cancels its integrable endpoint singularity. This reuses the same bounded panel
helper and retains a small tail before forming the lower complement. Shape-one
closed forms are applied to supplied log coordinates before normalization;
subtracting large log-gamma terms cannot erase their small exponent.

Modern beta uses a metered modified Lentz evaluation of the incomplete beta
fraction, with reflection, explicit convergence and a 100,000-step ceiling.
Endpoint coordinates can retain a logarithmic distance from one. This avoids
rounding a substantial transformed tail to zero before evaluation. Near-zero
coordinate asymptotics are used only below exp(-230), where the omitted next
term is negligible within the admitted shape envelope. Large-shape log weights
use `log1p(u)-u` and a compensated mean-relative numerator, rather than
subtracting large log-gamma values. The degree-32 `log1p(u)-u` series has a
geometric remainder bound for |u| <= 1/8.

Gamma uses charged positive lower series or upper continued fractions below
shape 4,096. Shape below one and x below one uses an alternating integrated
exponential series to retain a small upper tail. It does not subtract a lower
probability already rounded to one. Near the center, its log prefactor uses the
same cancellation-safe deviance and Stirling correction. Every iteration has an
explicit convergence test and finite cap.

For gamma shape at least 4,096, and beta with both shapes at least 1,000,000,
the kernels integrate the actual standardized density. A small private panel
helper compares 8- and 16-point Gauss–Legendre rules. It has a depth ceiling of
20 and a total allowance of 100,000 integrand evaluations, charging each before
work. It does not substitute a leading normal approximation. The relative
panel comparison has a floating-point noise floor, and failures to meet it
return a numerical convergence failure rather than a successful last estimate.
This is an adaptive error estimate, not a proof that every floating-point result
has that error. Independent high-precision controls supply the measured evidence.

Both integration boundaries come from Chernoff bounds, clamped to the actual
support. Gamma's exponent is `a*(log(x/a)+1-x/a)`. For beta, represent the
variable as X/(X+Y) for independent unit-scale gamma variables with shapes a,b;
Chernoff minimization gives `a*log(x/p)+b*log((1-x)/q)`, with p=a/(a+b), q=1-p.
The searches stop below exponent -800. Each omitted tail is therefore smaller
than half the smallest binary64 subnormal. Integrands are scaled near their
maximum on the selected side; final probabilities are formed in logs. A fixed
normal cutoff or absolute tolerance does not erase a representable tiny tail.

Binomial masses use beta log weights, avoiding factorial products and central
log-gamma cancellation. Intervals with at most 4,097 members are summed with
charged work; broader intervals use favorable lower/upper beta differences.
Hypergeometric masses use the conditional-binomial identity, avoiding three
large combinatorial values. Its CDF sums the selected side of the mean with
compensation and explicit per-member work. It does not silently approximate an
unbounded support sum. Exponential and Weibull CDFs use `expm1`; Weibull,
lognormal and transformed F arithmetic retain logarithmic scale information.

Shape parameters above 10^12, F degrees above 2*10^12, binomial/negative-binomial
counts above 10^12, hypergeometric population above 10^8, or hypergeometric sums
above 100,001 members exceed the current operational envelope. These are not
new Excel `#NUM!` domains: the evaluator reports an operational capability limit,
which IFERROR cannot catch. Host budget failures remain operational too.
Numerical failure to meet an iterative convergence test maps separately to
`#NUM!`. Domain errors are distinguishable in the pure kernel failure type.
No large output/support allocation is used.

## Independent accuracy evidence and provenance

Elementary tests use separately derived counting probabilities, Beta(1,1),
Beta(2,2), the arcsine beta integral, geometric negative-binomial probabilities,
shape-one exponential/gamma/Weibull identities, F(2,2), gamma half-integers and
normal integrals. Expected results never come from Poirot alone.

Additional mathematical data was generated with mpmath 1.3.0, verified BSD
3-clause license, [upstream tag](https://github.com/mpmath/mpmath/tree/1.3.0),
[license](https://github.com/mpmath/mpmath/blob/1.3.0/LICENSE). This is an external
reference calculator only, not a dependency or copied test suite. Independently
written generators use 90/130-digit tanh–sinh integration of the unrearranged
gamma density with high-precision logGamma, rather than the production deviance,
Stirling expression or Gauss panels. Their results agree to more than 70 decimal
digits. Beta controls similarly use 80/120-digit unrearranged density integrals
at the exact supplied binary64 coordinate. The tests include shapes 5*10^9,
asymmetric beta shapes 2*10^9 and 10^9, standardized offsets through ±30,
and a positive gamma tail around 3.74*10^-316. The generated mathematical values
are not adapted from upstream tests or MPL fixtures.

The Gauss–Legendre constants were independently generated at 90 digits by Newton
solving the Legendre recurrence; weights are `2/((1-x*x)*P'_n(x)^2)`. Only rounded
mathematical roots and weights appear in source. No foreign quadrature code or
coefficient table was copied. Development generators and detailed work/accuracy
logs are retained with the ticket evidence under `/tmp/poirot-distributions016`.
Existing libm 0.2.16 is MIT and supplies gamma, log gamma, erf and erfc through its
public APIs. No dependency implementation was copied or translated. Its existing
workspace lock version is reused; statrs' incomplete beta/gamma paths are not
called by these modern kernels.
