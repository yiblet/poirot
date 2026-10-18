# Positional SWITCH selection

Microsoft documents SWITCH as returning the result of the first matching case,
with an optional default and #N/A when no case or default applies:
https://support.microsoft.com/en-us/excel/functions/switch-function.
The sixteen authored SWITCH scenarios independently corroborated in Calc require
that selection at each retained array position before the aggregate consumes it.

Three options were considered:

1. A local first-match loop using existing rectangular readers, plus the concrete
   selection buffers and selected-result materialization already proved for IFS.
   Selected: these operations have the same demonstrated contract, without a
   generic selector planner or new persistent value state.
2. A separate array comparison kernel. Rejected: scalar SWITCH already owns typed
   equality, Excel numeric precision, blank admission, metering and operational
   capability failures. Reusing that exact implementation avoids a second policy.
3. Per-aggregate fixes, or reproducing Calc's unused-argument shape effects and
   padding variants. Rejected: these hide lost array information or add exceptions
   disproportionate to the defect. No aggregate changes are needed.

Under ExcelDocumentation, the expression and needed cases use inherited demand.
Scalar selection preserves the existing lazy reference and omission channels.
An explicit array, or a reference under array demand, starts positional selection.
Each position chooses the first matching result; remaining positions receive the
optional default or #N/A. Case and selected-result singleton axes broadcast.
Incompatible needed non-singleton extents produce #VALUE!, following IFS's bounded
rectangular contract. Needed argument expressions execute once, while reference
members are read only at unresolved/selected positions. The default is evaluated
only when selected. Selection scans, allocations, growth and output are metered.
Static spill bounds include expression, cases and results, retaining the existing
refusal of unbounded named/computed producers. PinnedPoi retains scalar execution.

The existing scalar comparison and error policies deliberately remain unchanged:
number/text/Boolean comparisons stay typed, equality uses pinned numeric
precision and blank-number admission, and Unicode equality can abort with the
existing operational capability failure. Scalar expression or case errors map to
#N/A; a spreadsheet failure produced by equality means no match. The array path
applies that same policy per position. This differs from Calc's broader SWITCH
admission: SUM(SWITCH({"1",1,TRUE},1,2,TRUE,3,9)) is 6 in Calc and 14 under
Poirot's existing typed policy. Calc also returns the original scalar expression
error and skips erroneous cases; Poirot retains its established #N/A policy.
These differences are recorded, not silently claimed fixed.

Calc inspects some unused argument shapes and pads some short selected operands.
For example SUM(SWITCH({1,1},1,2,3,{10,20,30},9)) is #VALUE! in Calc; Poirot
retains lazy selection and returns 4. A needed short non-singleton result is
rejected uniformly with #VALUE! rather than padded with #N/A. Explicit selected
omission remains MissingArgEval in scalar selection and retains the rectangular
reader's missing-to-blank limitation after materialization. No omission flags are
introduced. Physical blanks and empty text remain typed inside expressions;
worksheet publication alone normalizes a final blank to zero. Compound arguments
execute as a whole when needed, without a coordinate-lazy expression interpreter.

The diagnostic report retains all thirty original probes and disagreements.
Its zero expectations are placeholders, and RAND is an observation rather than a
stable numeric expectation. Fresh source-bound Calc observations corroborate all
48 checks in the changed production suite. No native Excel execution was available.
