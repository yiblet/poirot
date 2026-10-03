# Dynamic array results

`OverlayPolicy { arrays: ArrayResultPolicy::DynamicSpill, ..Default::default() }`
opts into worksheet spill publication. The default `ScalarIntersection` keeps
existing final top-left selection. Array publication is separate from builtin,
text, numeric, and formula-cache materialization policy; both exact configuration
and computation identities include it.

Use `overlay.evaluation(&context)` over source formula cells. The overlay owns
array policy and persistent footprint state; each evaluation owns execution state. A successful rectangular result owns its anchor and every child. Reading
a child first discovers bounded producers whose compiled footprint can cover
that coordinate. Sparse range consumers also discover intersecting producers and
include their children once, including when the anchor lies outside the range.
Source literals/formulas and input overlays take precedence over publication:
an occupied child, overlapping owned footprint, or worksheet boundary produces
`#SPILL!` at the anchor and publishes no children. Explicit blank cells allow a
spill; empty text and formulas returning empty text block it. Overlapping
rectangles are arbitrated in source-address order (row, then column): before an
anchor runs, earlier producers with intersecting static footprints run first.
The same rule applies recursively to overlap chains and to child discovery;
requesting an anchor or child first cannot change the winner. Overlap discovery
uses the normal depth and cell-work budget frames. Circular dependencies between
competing producers refuse before publication. Disjoint static
footprints remain unevaluated, and lazy branches retain ordinary selection. Admission reserves work, collection size, text, and combined memo
retention for the entire anchor/child batch before changing the memo.

Supported static geometry includes rectangular array constants, unary and binary
operators, TRANSPOSE, MMULT, MINVERSE, FREQUENCY, and bounded
IF/IFERROR/IFNA/CHOOSE branches. Operands can
be literal arrays, bounded cell ranges, scalar literals/expressions, or nested
bounded producers. Geometry discovery uses bounds captured during compilation, not expected values, and
does not evaluate unrelated producers with a known disjoint footprint. Lazy
branch selection remains in the evaluator. Under `ExcelDocumentation`, TRANSPOSE
preserves numbers, exact text/UTF-16 units, booleans, and individual errors;
reference blanks become numeric zero. Numeric matrix functions retain their
existing separate kernels and admission/coercion contracts.

Names, extension arrays, TREND, and computed-reference producers without a
supported static footprint bound are unsupported. Publication refuses a result
outside its syntax-derived bound. A child/range read that could lie southeast of an unbounded
potential producer conservatively refuses discovery rather than returning a
blank that could later become a child. Bounded lazy branches can still publish
inside their known enclosure without evaluating an unselected unbounded branch.
Direct and sparse reads of a producer's own unpublished footprint refuse with
an operational unsupported-capability failure.

Arithmetic (`+`, `-`, `*`, `/`, `^`), comparisons, concatenation (`&`), unary
plus/minus, and percent operate element by element. Scalars repeat across the
output, and a singleton row or column repeats along that axis. For example,
`{1,2}+{10;20}` spills `{11,12;21,22}`. Each output uses the existing scalar
numeric, text, and builtin policies, preserving individual error values. Both
operands are read before applying the scalar error rules, so even a right-hand
input paired with a left-hand error remains a dependency.

Selection follows the pinned POI `ArrayFunction` and `OperandResolver` sources:
output dimensions are the maximum of each operand dimension. Exhausting a
non-singleton axis produces `#VALUE!` for those elements; for example,
`{1,2}+{10,20,30}` produces `{11,22,#VALUE!}`. This mismatch behavior follows
POI rather than claiming Excel parity. References must resolve to one rectangular
sheet area with supported static geometry. Whole-row/column, computed-reference,
and reference-combination geometry is not inferred for operator publication.

Explicit `@` selects a scalar. CSE arrays and dynamic `#` reference syntax are
not implemented by this policy. Ordinary family scalar consumers retain their
existing contracts; this policy does not add general function broadcasting.

Overlay caches retain complete footprints, consumed input dependencies and child-to-producer
edges. Source writes, blockers, footprint changes and volatile inputs invalidate
producers, children and downstream consumers. Discovery also tracks possible
anchor membership: a new producer outside a consumed range can spill into it.
Cached footprints restore all children and ownership together after admission;
isolated cached child scalars are insufficient. Reuse never replaces a producer's
original dependency trace with an empty trace.

`changes()` and `result_changes()` represent saved child values with
`SetSpillCachedValue` and clear obsolete children with `ClearSpillCache`. Workbook application retains
prior authored blank/style metadata so clearing derived children restores source.
Workbook and overlay Serde snapshots preserve this distinction and ownership.
XLS/XLSX adapters do not currently encode complete dynamic-array ownership; file
round trips cannot be treated as retained runtime spill provenance.

Low-level ad hoc formula probes cannot publish arrays without a source cell.
External scalar-only cache ports still refuse dynamic publication, because they
carry no ownership. The primary overlay path supplies the full footprint contract.
Function broadcasting, external computed producers and the other capability
restrictions above remain explicit refusals rather than silent scalar fallbacks.

Excel specification fixtures select the optional execution setting
`"array_result_policy": "dynamic_spill"` explicitly. The runner expands assertion
targets but reads all values from the public overlay evaluation; it never constructs outputs
from expected arrays.
