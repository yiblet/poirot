# Documented Excel builtin contracts

`RunConfiguration::with_builtin_policy(BuiltinSemanticsPolicy::ExcelDocumentation)`
selects supported overrides derived from Excel documentation. It does not claim
an observed Excel version or backend parity. The default `PinnedPoi` policy keeps
the existing builtin registry, source contracts and extension precedence intact.
The text, numeric outcome and materialization policies remain independently
configured.

Supported profile-only aliases are `PERCENTILE.INC`, `QUARTILE.INC` and `RANK.EQ`.
The bounded family overrides below are also selected by this policy; other
builtins retain their existing contracts.
Names match ASCII case insensitively; an exact lowercase `_xlfn.` prefix is allowed
for these names. `PERCENTILE.INC` enters the existing typed inclusive percentile
handler. `QUARTILE.INC` has its own typed handler, truncates the selector into 0..4
and uses the same inclusive percentile kernel. `RANK.EQ` ranks ties equally,
interprets any nonzero order as ascending, and returns #N/A for a number absent
from the numeric sample. Both additions use the shared budgeted statistical host,
coercion and collection paths.

Compilation remains policy independent: otherwise unresolved calls are resolved
when executed, without cloning syntax or evaluating unselected lazy branches.
Already bound extensions retain their implementation and precedence. Profile-only
handlers have typed descriptors for invocation observations but are absent from
the default public catalog, which remains at 256 entries. Parsed and resolved
spellings are retained separately from the handler identity. An observation is
handler entry evidence, not a source-parity claim.

The builtin policy participates in both the complete configuration fingerprint
and its computation projection. Incremental reuse rejects results computed under
a different profile, including dependent cells. Runtime context changes clear
retained execution memos. Executed unresolved calls record `BuiltinPolicy`
consumption; its cache guard is the exact global computation identity. This global
guard also supports future overrides of already registered builtins safely.

`IPMT` and `PPMT` use their documented four-to-six-argument signatures in the
Excel profile. Optional future value and payment timing default to zero; timing
must be 0 or 1, and the period must lie between 1 and the total period count.
Periods retain fractional values. Zero-rate interest is zero and principal is
the constant payment. Beginning payments have zero first-period interest and
later interest follows the beginning annuity balance. Both functions retain the
financial adapter's resolve-before-coerce ordering for all supplied operands.
The pinned four-argument Finance algorithms and ignored PPMT extras are unchanged.
Sources: [IPMT](https://support.microsoft.com/en-us/excel/functions/ipmt-function)
and [PPMT](https://support.microsoft.com/en-us/excel/functions/ppmt-function).

`NORM.S.DIST` requires a cumulative selector in the Excel profile: true selects
the CDF and false selects the PDF. Negative standard-normal variates are valid.
The selector uses scalar boolean coercion, including cell references; an explicit
empty selector is false. The existing metered normal kernel preserves operational
budget faults. `NORMSDIST` and default-policy `NORM.S.DIST` keep their pinned
one-argument contracts.
Source: [NORM.S.DIST](https://support.microsoft.com/en-us/excel/functions/norm-s-dist-function).

The Excel profile also selects bounded differences in existing family adapters:
`ADDRESS` evaluates its fourth argument and formats absolute or relative R1C1
axes when false, retaining the shared sheet-name escaping. `CEILING.MATH` rounds
negative numbers away from zero for every nonzero mode. `FLOOR.MATH` rounds
negative numbers toward zero for every nonzero mode. This numeric direction
selection follows scalar argument admission; the pinned profile retains its
negative-only mode flags. Both profiles preserve omitted significance, explicit
missing-argument refusal and the zero-significance return before reading mode.
Basic `FLOOR(0,0)` returns zero in both profiles, as in its pinned POI source;
nonzero numbers with zero significance still return #DIV/0. `COVARIANCE.S` filters
nonnumeric positional pairs and returns #DIV/0 when fewer than two valid pairs
remain; mismatched lengths and input errors retain their typed failures. `RANK`
uses the shared `RANK.EQ` order, tie and absent-number behavior. Both accept
validated array operands or references through budgeted operand-kind inspection;
scalar and missing list arguments remain invalid. Pinned `RANK` retains its
reference-only admission. `ROMAN` distinguishes boolean form
values from numeric styles: TRUE selects classic form 0 and FALSE simplified
form 4. The pinned entry points retain their source contracts. All overrides use
shared scalar/reference hosts, collection budgets and existing numeric kernels.
Sources: [ADDRESS](https://support.microsoft.com/en-us/excel/functions/address-function),
[CEILING.MATH](https://support.microsoft.com/en-us/excel/functions/ceiling-math-function),
[FLOOR.MATH](https://support.microsoft.com/en-us/excel/functions/floor-math-function),
[COVARIANCE.S](https://support.microsoft.com/en-us/excel/functions/covariance-s-function),
[RANK.EQ](https://support.microsoft.com/en-us/excel/functions/rank-eq-function),
and [ROMAN](https://support.microsoft.com/en-us/excel/functions/roman-function).

Array result publication is selected independently with `ArrayResultPolicy`.
See [dynamic array results](dynamic-array-results.md) for footprint ownership,
static discovery bounds, atomic memo admission, and unsupported execution modes.

`SUBTOTAL` reference operands are supported in the Excel profile through a
budgeted metadata reader and the existing aggregation kernel. Any referenced
formula containing a parsed `SUBTOTAL` call is excluded before evaluation;
text resembling a call is ordinary data. Filtered rows are excluded for all
function codes, while manually hidden rows are excluded for codes 101 through
111. `Worksheet::set_row_visibility` declares these independent sparse flags;
omitted metadata means visible. Current file adapters do not retain row
visibility, so imported workbooks must have relevant visibility declared by the
caller. This is not a claim of hidden-row import fidelity.

Run overlays replacing a formula with a value remove its nested-formula status
and retain the baseline row visibility. Metadata reads are separately observed
from value reads, including excluded cells. Exact source-formula and visibility
guards invalidate incremental results, while plan identities include visibility
and reject stale source views. The default pinned reference refusal is unchanged.
Source: [SUBTOTAL](https://support.microsoft.com/en-us/excel/functions/subtotal-function).

The date overrides return #NUM for negative DATE years, normalize through the
1900 fictitious leap-day slot, retain fractional numeric DAYS subtraction,
truncate signed WORKDAY offsets toward zero, and handle ISO WEEKNUM week-year
rollover. CODE, IMREAL and IMAGINARY return numeric coefficients/results;
BIN2DEC validates binary digits and DELTA defaults its omitted second operand to
zero. These changes preserve default source implementations and do not establish
exhaustive date, text encoding or engineering compatibility. The package-local
[inventory](../../poirot-test-excel-specs/test-specs/inventory.json) records supported
policies, individual source links and remaining coverage obligations.
