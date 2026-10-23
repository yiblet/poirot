# Prepared dynamic spills

`ArrayResultPolicy::DynamicSpill` resolves possible owners when a demanded
coordinate is eligible for incoming spills, or when a range needs ownership
completeness. Authored formulas and nonblank literals block incoming spills and
ordinary reads evaluate them directly. Eligible coordinates are retained cache
nodes: they depend on their possible owners and on the spatial query that found
them. An empty query is still a dependency, so inserting a new possible owner
invalidates a previously blank read. Public dependency observations name that
coordinate as the consumer; producer completion notifications keep their owner.

Formal result types and independent row/column bounds define possible
rectangles on each sheet. Unknown dimensions
extend down/right to the grid edge. A source-owned index retains these proofs
across evaluations; source, parser and registry changes invalidate classification.
Cold classification still inspects and meters effective stored formulas. A read
outside all possible rectangles executes no producer, and a fixed bound excludes
provably disjoint cells. Possible rectangles never reserve cells. Ordered candidate
starts and a prefix maximum of ending rows bound each query before exact rectangle
checks. Disjoint short rectangles avoid rescanning every earlier producer. A tall
or unknown-height rectangle can still require linear tests; earlier-competitor
closure can compare many pairs even when actual footprints are disjoint.

Preparation includes earlier candidates whose possible rectangles compete with
selected producers. Dependencies can extend that component, including across
sheets. Actual footprints, rather than possible bounds, decide ownership. Poirot
uses strict top-left priority: lowest row, then lowest column, always wins an
actual overlap. This is an explicit Poirot policy, not a claimed Excel tie-break.
Authored nonblank cells and formulas remain blockers; a losing array publishes
only its `#SPILL!` anchor.

An actual supplier can lose to its dependent consumer. For example, D1 spills
five rows and adds A6, while A5 spills two rows through column D. D1 wins at D5,
A5 becomes `#SPILL!`, A6 is blank, and D1:D5 settle to 1 through 5. If D1 actually
has only two rows, both spills fit and A6 supplies its ordinary value. Conservative
possible overlap alone does not establish a cycle or block either producer.

The evaluator stages this dependency component privately. It computes under an
assumed set of losing producers, arbitrates actual rectangles in priority order,
and retries only when that losing set changes. Losing producers retain their
attempted geometry for arbitration but expose no children to the next attempt.
A stable assignment therefore uses the same supplier values as final ownership.
Repeated component-and-losing-set states, direct self-footprint reads and
actual producer cycles fail explicitly. A growing dependency component is a new
state. Refusing a periodic sequence does not claim to solve all circular equations
or prove that no other mathematical fixed point exists. Work, temporary allocations
and retries remain metered; numerical rules are unchanged.

Retries remove only the attempt's admitted cells and dependency evidence. Earlier
settled public results on the same sheet remain intact. Captured clock values
stay fixed; cell-local random streams and draw ordinals rewind while actual work
stays charged. Actually consumed opaque or volatile external provider contracts
prevent replay: if another attempt is necessary, evaluation fails before calling
that provider again. An unused external branch does not prevent settlement.
Builtin recalc volatility alone does not make immutable-context operations unsafe
to repeat. Execution/invocation traces describe actual attempts; cell completion
callbacks expose only the final settled attempt, or operational failure.

A completed range retains one run-local readiness proof for its member reads.
The proof requires successful discovery and actual settlement, stays unavailable
during a private attempt or after failure, and resets with the evaluation cohort.
Point reads preserve the enclosing range. Source inspection alone never seals it.
The component workspace reuses empty hash-table storage between independent
components; selected owners, replay effects and notifications do not survive.

Ordinary reads then consume cached values and generated children directly.
Sparse ranges, lookup keys and return columns share this view. Lookup indexes
require all possible owners of the full searched area to have settled. Ordered
reduction buffering, reuse and retention use the same area completeness proof;
no provisional range enters a reusable reduction. Persistent directional guards
also cover missing cells, so later growth invalidates consumers and indexes.
Source value changes and geometry changes have distinct invalidation paths. Dirty
coordinate reads validate their potential owners through the ordinary dependency
engine; unchanged coordinate values cut off downstream recalculation. Complete
retained producers can validate several dirty dependencies without executing
their formula when all consumed results remain identical. Their array payload,
shape and ownership are retained together. Owned child reads still restore the
complete footprint through priority preparation before reuse, preserving original
dependency evidence and indexes when unchanged. Source metadata edits force
direct readers conservatively; geometry changes also force sparse interval
consumers. This does not remove private overlap arbitration or make child reads
settlement-free.
New geometry invalidates intersecting consumed lookup prefixes. A sparse journal
carries admitted ownership to Overlay publication. A failed component discards
its new admissions and poisons further dynamic reads; the existing successful-call
transaction prevents a failed output batch from publishing partial results.

The integration examples in `tests/spill_preparation.rs` are original tests. Their
array layout and blocker expectations follow Microsoft's dynamic-array documentation:
https://support.microsoft.com/en-us/excel/dynamic-array-formulas-and-spilled-array-behavior
No external code, algorithms, tests or fixtures were copied or adapted.

The postfix `#` operator references the finalized footprint of one anchor. It is
available to scalar consumers such as `SUM(A1#)`, reference consumers such as
`ROWS(A1#)`, and forwarding roots such as `D1=A1#`. A dependency on an anchor
finalizes that supplier before its extent is inspected. One-cell arrays retain an
anchor role; ordinary scalar cells and ordinary single-cell reference roots do not.
Area reference roots materialize through the same rectangular publication path.
Existing signed reference channels use that path too; `#` requires a physical
single-cell anchor, so off-grid signed operands produce `#REF!`. Multi-area and
multi-sheet roots retain existing scalar projection and do not introduce a new
spill layout. Singleton arithmetic, unary operations and IF condition arrays
retain their actual array channel until their consumer selects a scalar; the
ScalarIntersection publication policy continues to select one public value.

Microsoft documents complete, growing and shrinking range references:
https://support.microsoft.com/en-us/excel/spilled-range-operator
The geometry of error-containing references follows from that contract and the
Microsoft ROWS reference/array contract; this is an inference from those contracts,
not a live Excel observation:
https://support.microsoft.com/en-us/excel/functions/rows-function
Poirot explicitly returns `#REF!` for a non-single-cell operand or an anchor without
an accepted footprint, including generated children. If an unaccepted anchor has
an error, that error propagates. Accepted footprints keep their reference geometry
even when their first member is an error. These edge choices and one-cell identity
are Poirot contracts where the inspected Microsoft pages do not specify details.
External workbook references remain outside the supported parser/reference surface.

The ExcelDocumentation profile accepts `ANCHORARRAY` and `_xlfn.ANCHORARRAY`
through the same resolver. The pinned POI function inventory is unchanged. XLSX
export rewrites parsed postfix syntax to `_xlfn.ANCHORARRAY` without changing text
literals. The format evidence is the installed rust_xlsxwriter 0.101.0 Formula
writer-author documentation, section “The Spilled Range Operator #”, and its
future-function table; it is not Microsoft normative format proof:
https://docs.rs/rust_xlsxwriter/0.101.0/rust_xlsxwriter/struct.Formula.html
No external implementation, tests or fixture data were copied or adapted.

Saved source metadata records an anchor's declared lower-right corner separately
from its children and binds it to the exact formula spelling. This represents
one-cell arrays and incomplete sparse caches. `Worksheet::complete_spill_extent`
checks full child ownership without expanding missing coordinates; saved geometry
alone does not authorize evaluator reuse. Source edits invalidate extent authority.
Snapshots retain valid declarations, validate all serialized coordinates, and drop
stale declarations after raw source changes. XLSX import records only array-formula
`ref` attributes, never shared-formula bounds. Export preserves declared extents,
and legacy manually attached children can still supply their bounding rectangle.
Serialized source, compiled plans, builtin contracts and Overlay state carry
compatibility versions; incompatible positional payloads reject explicitly.

`ProducerPolicy::Compute` remains the default. With `DynamicSpill`, the explicit
`ProducerPolicy::TrustSaved` classifies the same possible producers but consumes
their saved source values without executing their expressions. Ordinary scalar
formulas still calculate normally. Configure it with
`RunConfiguration::with_producer_policy`, `OverlayPolicy::producers` on the direct
or Overlay builders, or `Workbook::recalculate_with_policy` in the public facade.
`ScalarIntersection` continues ordinary expression evaluation.

This trust is the caller's freshness promise. Even a possible producer that
currently returns a scalar can retain an intentionally stale clock/random result;
ordinary scalar NOW/TODAY consumers keep precise captured-context invalidation.
Saved scalar caches have no presence bit: an omitted cache and an explicit Blank
are indistinguishable and both return Blank. No geometry and no owned children
means a scalar cache. A declared array requires every owned member of its exact
rectangle; incomplete declarations or legacy owned children without declarations
fail with an attributed UnsupportedCapability, without recalculating. A complete
saved 1x1 array keeps its reference identity, including error/Blank members.

Restoration reads cached children through the source cache view and preserves
separate authored cells. Content replacement at an anchor uses that replacement's
own saved scalar and cannot revive its baseline children. An effective content
edit at a saved child fails restoration; style-only edits retain saved content.
Authored blockers are never overwritten. A removed producer releases old children.
Dependency-validated live Overlay results remain complete units under both policies;
source edits, ownership changes, context consumption, and policy changes invalidate
those units and dependent indexes. Trusted source results record structural guards
but acquire no fictitious expression/context reads. Newly restored units use the
same admission journal as computed results; skipped expressions do not increment
formula execution counters and completion observers report ExternalCache origin.
Authored style-only markers preserve edit meaning through native snapshots,
source recovery and flush. Content edits dominate later style changes; an explicit
Blank assignment cannot be reinterpreted as a style edit. Restored markers require
an existing, nondeleted override with exactly unchanged baseline content. Model
style updates retain saved ownership and the authored style beneath child caches.
Native Overlay snapshots version policy, edit metadata and dependency evidence.
XLSX and source snapshots retain saved extents and ownership. Incompatible
Overlay revisions reject explicitly.
