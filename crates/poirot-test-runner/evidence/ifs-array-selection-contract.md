# Positional IFS selection

Microsoft documents IFS as returning the result associated with the first true
condition, and #N/A when no condition succeeds:
https://support.microsoft.com/en-us/excel/functions/ifs-function.
The original sixteen independently authored scenarios, corroborated in Calc,
require that rule at each array position before ordinary aggregates consume the
result. No aggregate changes are needed.

Three options were considered:

1. A local first-true traversal using the existing rectangular readers and
   deferred arguments. Selected: it follows the existing lazy selection model.
2. A generic selection framework shared with IF and future selectors. Rejected
   here: the probes show different shape behavior and no shared selection contract
   has been established.
3. Per-aggregate fixes or copying IF's padding rule. Rejected: these conceal the
   lost array channel or contradict independent IFS observations.

Under ExcelDocumentation, scalar conditions retain the established lazy result
and reference channel. Once a condition retains an array, IFS tracks the first
matching result ordinal at each position. Only unresolved positions read later
condition members; only selected result members are dereferenced. Each needed
condition or result expression is evaluated once. Singleton axes broadcast.
Needed operands with unequal non-singleton extents are rejected with #VALUE!.
Selection buffers, growth, scans, branch storage and output copies are metered.
PinnedPoi keeps its previous scalar condition contract.

This intentionally bounded implementation does not promise complete Calc shape
parity. Calc sometimes lets unused operands enlarge the result or cause a shape
error. For example, SUM(IFS({TRUE,TRUE},2,TRUE,{10,20,30})) yields #VALUE! in
Calc; Poirot retains lazy selection and returns 4. A scalar early true still
forwards its selected reference without evaluating later shapes. Calc also
pads some exhausted selected results with #N/A; Poirot rejects incompatible
extents uniformly with #VALUE!, rather than adding a second padding convention.
Both differences are retained in the diagnostic report, not hidden by changed
corpus expectations.

Scalar omission remains the existing MissingArgEval channel. Materialized array
members retain the existing rectangular reader's missing-to-blank limitation;
COUNT(IFS({TRUE,FALSE},,TRUE,3)) yields 2 in Calc, while Poirot's bounded array
model yields 1. No persistent omission flags or new workbook value states are
introduced. Physical blanks and empty text are retained until consumption;
worksheet formula publication continues to normalize final blank results to zero.

The probes also retain SUM(reference-array-condition) versus SUMPRODUCT's array
demand difference. No global aggregate demand change is made. Compound branch
expressions evaluate as a whole once if needed somewhere; this is not a
coordinate-lazy expression interpreter. Static spill bounds include conditions
and result syntax, with the existing refusal of unbounded named/computed
producers. No native Excel execution was available.
