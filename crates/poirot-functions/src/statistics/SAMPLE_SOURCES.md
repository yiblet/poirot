# Modern sample statistics contracts and independent evidence

The 21 modern sample functions use first-party implementations of elementary
mathematical equations. No third-party source code, algorithm implementation,
test, expected-data table or MPL FODS fixture was copied, translated or adapted
into these MIT library additions. Existing admitted modern beta/gamma tail
implementations and libm are reused with their existing provenance. The external
mpmath 1.3.0 calculator is BSD-3-Clause
([tagged source](https://github.com/mpmath/mpmath/tree/1.3.0)); it only evaluates
independently written defining-integral expressions in temporary evidence. Its
license and exact calculator identity are retained with that evidence. It is
neither a runtime dependency nor a copied algorithm.

Primary contracts checked on 22 October 2026:

| Name | Microsoft source | Implemented contract |
| --- | --- | --- |
| CHISQ.TEST | [page](https://support.microsoft.com/en-us/excel/functions/chisq-test-function) | Two equal rectangles; supplied expected frequencies, table/vector degrees, upper gamma tail. |
| F.TEST | [page](https://support.microsoft.com/en-us/excel/functions/f-test-function) | Independent numeric sets; nonzero sample variances; twice the smaller F tail. |
| FISHER | [page](https://support.microsoft.com/en-us/excel/functions/fisher-function) | atanh on the open unit interval. |
| FISHERINV | [page](https://support.microsoft.com/en-us/excel/functions/fisherinv-function) | Saturating tanh for finite inputs. |
| HARMEAN | [page](https://support.microsoft.com/en-us/excel/functions/harmean-function) | Positive admitted numbers; scaled reciprocal mean; 1..255 arguments. |
| KURT | [page](https://support.microsoft.com/en-us/excel/functions/kurt-function) | Bias-corrected excess fourth moment; at least four members, nonzero spread; 1..255 arguments. |
| MODE.MULT | [page](https://support.microsoft.com/en-us/excel/functions/mode-mult-function) | All repeated modes in a vertical spill; NA if no repeated number; 1..254 arguments. |
| MODE.SNGL | [page](https://support.microsoft.com/en-us/excel/functions/mode-sngl-function) | First encountered numeric mode; distinct modern descriptor and admission; 1..254 arguments. |
| PERCENTILE.EXC | [page](https://support.microsoft.com/en-us/excel/functions/percentile-exc-function) | Interpolated one-based position (n+1)k, permitted only within [1,n]. |
| PERMUT | [page](https://support.microsoft.com/en-us/excel/functions/permut-function) | Truncated counts n>0, 0<=k<=n; falling product bounded by proved binary64 overflow. |
| PERMUTATIONA | [page](https://support.microsoft.com/en-us/excel/functions/permutationa-function) | Truncated nonnegative counts; n^k; repetitions allow k>n. |
| PROB | [page](https://support.microsoft.com/en-us/excel/functions/prob-function) | Positional masses in (0,1], total one, inclusive bounds, omitted upper equals lower. |
| QUARTILE.EXC | [page](https://support.microsoft.com/en-us/excel/functions/quartile-exc-function) | Truncate quart to 1..3; exclusive percentile at quart/4. |
| RANK.AVG | [page](https://support.microsoft.com/en-us/excel/functions/rank-avg-function) | Average exact equal ranks, descending order0, ascending any other order. |
| RSQ | [page](https://support.microsoft.com/en-us/excel/functions/rsq-function) | Squared correlation on aligned admitted pairs. |
| SKEW | [page](https://support.microsoft.com/en-us/excel/functions/skew-function) | Corrected sample third moment, at least three members/nonzero spread; 1..255 arguments. |
| SKEW.P | [page](https://support.microsoft.com/en-us/excel/functions/skew-p-function) | Population third moment; documented minimum three/nonzero spread; 1..254 arguments. |
| STEYX | [page](https://support.microsoft.com/en-us/excel/functions/steyx-function) | sqrt(fitted Y residual sum of squares/(n-2)); at least three aligned pairs. |
| T.TEST | [page](https://support.microsoft.com/en-us/excel/functions/t-test-function) | Paired, pooled or Welch statistic; truncated tails1/2 and type1/2/3. |
| TRIMMEAN | [page](https://support.microsoft.com/en-us/excel/functions/trimmean-function) | Remove floor(n*percent/2) members from each end; percent in [0,1]. |
| Z.TEST | [page](https://support.microsoft.com/en-us/excel/functions/z-test-function) | Signed normal survival; omitted sigma uses sample deviation, actual zero is DIV0. |

The Office [TTEST clarification](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/2cdcacbb-5106-45f0-bf6a-22a240164164)
specifies NUM for invalid type. Worksheet Student-degree truncation is not used
for Welch degrees.

## Independent equations and controls

Center against a member before normalizing; when raw subtraction overflows,
zero is a safe anchor at the dominant separation scale. Summation is compensated.
Dimensionless third/fourth moments retain normalized sums rather than requiring
an overflowing variance. Each paired coordinate is scaled independently;
STEYX sums fitted residual squares directly, avoiding cancellation in Syy-Sxy²/Sxx.
Two-group means retain compensated high/low parts so nearby large means are
not rounded equal before subtraction. Their actual difference components are
scaled after large anchor/offset cancellations, preserving a tiny hypothesized
mean beside a wide symmetric sample. Rescaled product residuals preserve raw
below-subnormal mean offsets. Variances, standard errors and tail coordinates
retain logs.

Translation/scaling of [0,0,1] gives sample SKEW=sqrt(3), population SKEW=1/sqrt(2).
Translation/scaling of [0,0,0,1] gives KURT=4; [-1,-1,1,1] gives KURT=-6.
Tests use ordinary integers, huge/tiny spreads and adjacent large finite values.
For x=[0,1,2], y=[1,0,1], correlation is zero and SSE=2/3, so STEYX=sqrt(2/3).
A line with huge Y and tiny X has RSQ1 and zero residuals. HARMEAN(2,6)=3;
exclusive percentile at3/8 on [0,10,20] is5; RANK.AVG of2 on [2,2,7] is2.5
in descending order. Exact binary fractions test PROB without a tolerance issue.

Welch x=[0,2], y=[2,3,4] gives t²=3, nu=32/19 and two-tail
I_(32/89)(16/19,1/2)=0.24818853808401208689…. Independent 80/120-digit defining
beta integrals agree beyond70 digits. Paired differences [-1,-2,-3] give
two-tail 1-sqrt(6/7). F with df2/df2 and variance ratio4 gives2/5. A2x2 table
with diagonal3/off-diagonal1 and expected2 has statistic2, df1 and erfc(1);
a four-member vector instead has df3. Signed Z controls use the normal erfc
integral, including raw binary64 subnormals in pure tests. Public ingestion
currently flushes supplied subnormals before function admission.

## Explicit policies and native uncertainties

These choices are deterministic implementation policies where primary pages
are incomplete or internally inconsistent; comprehensive native Excel
origin/error/default evidence remains in the compatibility audits:

- Modern modes retain first-occurrence order and canonical numerical signed zero.
  An ascending-looking Microsoft example does not establish sorted tie order.
  GROUPBY MODE/MODE.SNGL share the modern first-occurrence numeric kernel
  and origin admission, with operational allocation failures. Pinned worksheet MODE keeps its old descriptor,
  coercion and implementation.
- Variadic moments/HARMEAN/modes count direct logical and numeric text, ignore
  them in arrays/references, and propagate errors. Direct blanks are ignored;
  explicit missing required/variadic members return VALUE. RSQ/STEYX use that
  origin policy positionally. F.TEST/T.TEST/Z.TEST and ordered collection inputs
  retain actual numbers; errors propagate. Exact short-page direct-origin and
  missing-versus-blank precedence remains to be confirmed natively.
- CHISQ.TEST requires equal rectangular shape, every position numeric,
  nonnegative observations/expected values and positive expected denominators.
  Geometry is preserved rather than silently dropping table positions.
- PROB requires actual numbers at every position and equal flattened lengths;
  it never independently filters x and probability arrays. Compensated mass sum
  must equal one exactly; native tolerance is unverified. Its page calls lower
  optional without specifying a default: omitted lower explicitly defaults0.
  Actual blank lower is also0. Reversed bounds yield0 after mass validation.
- RANK.AVG returns NA for an absent target and uses exact equality, with no
  epsilon tie or INT snapping. The separate old RANK diagnostic remains an
  upstream arithmetic-origin audit, not justification for changing ties.
- QUARTILE.EXC follows explicit endpoint errors despite a contradictory later
  MIN/MAX sentence. PERMUTATIONA rejects negative truncated counts, accepts
  zero bases and zero exponents, including0^0=1. These short-page corners remain
  native audit items. Constant-X STEYX returns DIV0; constant-Y with variable X
  returns0; RSQ requires spread in both coordinates.
- Every collection and per-cell lifted recomputation is charged before work.
  Mode/output allocation failures and admitted tail work failures remain
  operational, outside IFERROR. T.TEST has no arbitrary worksheet degree cap;
  admitted beta/gamma finite implementation envelopes and numerical
  nonconvergence remain the existing typed operational/nonconvergence channels.
  Scalar bounds/selectors lift; collections and positional pairs never broadcast.

All XLSX spellings were already present in the writer's future-name ledger;
this ticket preserves legacy unprefixed spellings and modern _xlfn spellings.
No workbook model/source schema or artifact envelope change is needed. Registry,
portable compiler, raw Overlay and compatibility revisions invalidate stale
binding, shape and result facts; source/artifact recovery preserves edits.
