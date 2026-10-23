# Legacy statistics worksheet contracts

Reviewed 2026-10-22 against Microsoft's legacy function documentation. These
contracts select existing Poirot numerical kernels; no upstream code, test data
or numerical algorithm was copied or adapted for this delivery. The tests use
independent equations, counting and documented boundary rules. Native Excel was
not executed. Original segregated LibreOffice FODS diagnostics are comparison
evidence only and supplied no code, data, fixtures or algorithms to these tests.

Each legacy spelling has a distinct parser and registry identity. This preserves
its actual arity, typing and stored XLSX formula spelling, independently of modern
registration order. Availability is the ExcelDocumentation profile; PinnedPoi
retains its prior missing-name outcomes.

| Microsoft primary contract | Worksheet selection |
| --- | --- |
| [BETADIST](https://support.microsoft.com/en-us/excel/functions/betadist-function) | Lower beta CDF, 3–5 arguments; slots 4 and 5 are bounds with defaults 0 and 1. |
| [BETAINV](https://support.microsoft.com/en-us/excel/functions/betainv-function) | Lower beta inverse, 3–5 arguments; probability excludes 0, includes 1; optional bounds. |
| [BINOMDIST](https://support.microsoft.com/en-us/excel/functions/binomdist-function) | Binomial mass or lower CDF, required selector. Truncated counts. |
| [CHIDIST](https://support.microsoft.com/en-us/excel/functions/chidist-function) | Upper chi-square tail; truncated degrees in [1,10^10]. |
| [CHIINV](https://support.microsoft.com/en-us/excel/functions/chiinv-function) | Upper chi-square inverse; truncated degrees at least 1; numerical exhaustion is NA. |
| [CHITEST](https://support.microsoft.com/en-us/excel/functions/chitest-function) | Rectangular chi-square test. |
| [CONFIDENCE](https://support.microsoft.com/en-us/excel/functions/confidence-function) | Normal mean-confidence radius; sample size truncated, at least 1. |
| [CRITBINOM](https://support.microsoft.com/en-us/excel/functions/critbinom-function) | Least count with CDF at least alpha; success probability and alpha strictly between 0 and 1. |
| [EXPONDIST](https://support.microsoft.com/en-us/excel/functions/expondist-function) | Exponential rate, required density/CDF selector. |
| [FDIST](https://support.microsoft.com/en-us/excel/functions/fdist-function) | Upper F tail; both truncated degrees in [1,10^10). |
| [FINV](https://support.microsoft.com/en-us/excel/functions/finv-function) | Upper F inverse; both truncated degrees in [1,10^10); numerical exhaustion is NA. |
| [FTEST](https://support.microsoft.com/en-us/excel/functions/ftest-function) | Two-sided variance-test probability. |
| [GAMMADIST](https://support.microsoft.com/en-us/excel/functions/gammadist-function) | Gamma scale, required density/CDF selector. |
| [GAMMAINV](https://support.microsoft.com/en-us/excel/functions/gammainv-function) | Lower gamma inverse, rejects text/missing parameters; numerical exhaustion is NA. |
| [HYPGEOMDIST](https://support.microsoft.com/en-us/excel/functions/hypgeomdist-function) | Fixed hypergeometric mass, four truncated counts; sample and population success counts must be positive. |
| [LOGINV](https://support.microsoft.com/en-us/excel/functions/loginv-function) | Lognormal inverse with log-space mean and deviation. |
| [LOGNORMDIST](https://support.microsoft.com/en-us/excel/functions/lognormdist-function) | Fixed lognormal CDF, three parameters. |
| [NEGBINOMDIST](https://support.microsoft.com/en-us/excel/functions/negbinomdist-function) | Fixed negative-binomial mass, three parameters. |
| [QUARTILE](https://support.microsoft.com/en-us/excel/functions/quartile-function) | Inclusive quartile, truncated selector 0 through 4. |
| [TINV](https://support.microsoft.com/en-us/excel/functions/tinv-function) | Nonnegative two-tail Student inverse; truncated degrees at least 1; numerical exhaustion is NA. |
| [TTEST](https://support.microsoft.com/en-us/excel/functions/ttest-function) | Paired/equal-variance/Welch test, truncated tails/type selectors. |
| [WEIBULL](https://support.microsoft.com/en-us/excel/functions/weibull-function) | Weibull scale, required density/CDF selector. |
| [ZTEST](https://support.microsoft.com/en-us/excel/functions/ztest-function) | Signed upper normal probability; omitted deviation uses sample deviation. |

## Declared limits and unverified native details

The modern pure kernels retain their existing finite work envelopes and bounded
searches. This does not reproduce every historical iteration strategy documented
by Microsoft. Numerical nonconvergence is a catchable spreadsheet error; budget
and computational-envelope failures remain operational and bypass IFERROR.
CHIINV and FINV at probability 0, and GAMMAINV at probability 1, have unbounded
mathematical quantiles; Poirot retains NUM for their finite representation policy.
This is not a native endpoint observation. TINV maps exhausted searches to NA
without changing modern T.INV.2T's NUM policy.

Scalar parameters use the existing explicit numeric origin/coercion rules and
rectangular lifting. GAMMAINV retains the text/missing guard, covering both stored text representations for the legacy spelling. The existing modern text policy remains unchanged. CHITEST
requires identical rectangular geometry and strictly numeric positions; Microsoft
alone does not resolve every shape/origin detail. FTEST and TTEST retain numeric
sample admission and paired positions; reference/array text, logicals and blanks
are excluded, zeros retained. QUARTILE shares existing inclusive collection
admission. ZTEST retains existing sample/sigma domain choices where the legacy
page does not settle every origin or error detail. No broader native equivalence
is claimed by these registrations or the diagnostic FODS results.
