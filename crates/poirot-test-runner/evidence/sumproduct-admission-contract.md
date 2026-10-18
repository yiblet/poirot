# SUMPRODUCT singleton admission

The saved upstream worksheet and fresh Calc both produce zero for the original
single-cell text-area product. Microsoft documents conformable dimensions and
nonnumeric entries treated as zero:
https://support.microsoft.com/en-us/excel/functions/sumproduct-function

Three options were compared:

1. Remove the singleton shortcut. Rejected: the same text/blank mistake survives
   for scalar references, direct values, and mixed-origin later arguments.
2. Select the existing rectangular schedule for every documented-profile argument,
   including singletons, and request the existing centralized text/blank-zero
   member admission. Selected: the same rule covers every tested origin and order
   without new value tags, operand descriptors, or first-argument special cases.
3. Introduce richer origin metadata or source-cell exceptions. Rejected: existing
   shape/origin descriptors already retain all information this rule needs.

All arguments must have the same dimensions; scalars do not broadcast. Modern
shape mismatch rejects before area-cell traversal. Equal shapes use ordered,
row-major member reads, propagate spreadsheet errors, and preserve host aborts.
The pinned POI scalar/area dispatch and error traversal remain unchanged.
BuiltinPolicy dependencies in both profiles protect retained-result invalidation;
raw snapshot version and compatibility revision advance with execution semantics.

`sumproduct-admission-probes.json` retains all 188 independent observations,
including disagreements and the input manifest. Text/blank zero admission agrees
across direct values, scalar references, explicit one-cell areas and arrays,
larger ranges, mixed origins, and both argument orders. Explicit -- converts
numeric text and logical values before SUMPRODUCT consumes them.

This is a bounded text/blank fix, not a claim of complete nonnumeric compatibility.
Calc admits uncoerced booleans in arrays; Microsoft's nonnumeric wording does not
resolve every direct/reference/array logical case without stronger target evidence.
The existing Boolean-to-number behavior is retained, with no new assertions that
native Excel agrees. Calc also admits omitted arguments as zero, while Poirot
retains explicit Missing rejection. Calc scalar errors can precede shape rejection;
this change does not add an origin-specific shape/error exception. No native Excel
execution was available. These controls remain observations rather than new target
expectations.

`sumproduct-admission-libreoffice.json` binds the changed original suite by SHA-256:
149 scenarios and 149 checks corroborated. Only the owned known-gap annotation is
removed; original source expectations, formulas, and tolerances are preserved.

Valid UTF-16 numeric text follows the existing bridge projection and the same
text/blank admission. Malformed UTF-16 projection remains operationally unsupported
before member admission, including after an earlier zero term; IFERROR does not
catch that host failure. No text representation rewrite is attempted.
