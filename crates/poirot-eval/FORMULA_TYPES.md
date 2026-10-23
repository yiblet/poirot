## Formula type contract

Preparation interprets existing expression syntax and resolved function identities.
It retains a possible type at every executable `Enter` boundary, including lazy
argument blocks. The first boundary describes the prepared cell's root expression.
No extra type/effect payload is attached to a runtime value. Public mutable source
cells have no authoritative cached annotation: exact source and binding identities
protect the existing prepared-formula cache. Names remain unresolved until scoped
workbook resolution, so their static result is top.

The finite grammar is:

```text
K ::= subset of {Blank, Number, Text, Boolean, Error}
T ::= Scalar(K) | Array(K) | Ref1(K) | RefN(K) | Missing | T ∨ T
E ::= (possible host-read categories, external behavior)
d ::= positive integer | ∞
D ::= (d, d)
P ::= (T, D) with E
external behavior ::= Tracked ≤ Volatile ≤ Opaque
```

`FormulaType` stores the union in five components. Empty scalar-kind sets mean an
absent alternative. Bottom has no alternatives. Top has every scalar kind in each
form plus Missing. `Ref1` describes a single coordinate. `RefN` allows one or more
coordinates, possibly multiple areas or sheets. A single-cell source reference is
therefore not spillable merely because its expression returns a reference. Concrete
shapes, coordinates and stored values remain runtime data; static upper bounds
restrict possible materialized dimensions without establishing ownership.

The concretization γ maps a scalar component to scalar values of its kinds, an
array component to nonempty rectangular values whose elements have those kinds,
and a reference component to references with the described cardinality and possible
stored element kinds. Missing admits the existing omitted-argument value. Both text
encodings inhabit Text; every binary64 pattern inhabits Number. Spreadsheet errors
inhabit Error; operational aborts are outside the successful-result judgment.
γ(T) is the union of those alternatives. Array element abstraction deliberately
admits mixed arrays after a join: γ(A ∨ B) can be larger than γ(A) ∪ γ(B).

Ordering is componentwise subset (and false ≤ true for Missing). Join is componentwise
union. It is associative, commutative and idempotent, and is the least upper bound
in this abstract ordering. Effects use subset for existing `DependencyPolicy`
categories and the displayed external-behavior order; their join is componentwise.
Opaque subsumes Volatile because both disallow retained-result reuse and Opaque also
lacks a complete input/effect contract. This ordering grants no call-reordering right.
Captured clock or random seed dependence is a tracked characteristic, not external
volatility.

The typing judgment is `Γ ⊢ e : T with E`, where Γ contains resolved builtin/extension
contracts and source binding identity. Literals introduce their scalar kind; omitted
arguments introduce Missing. Cell references introduce Ref1(AnyScalar), with a possible
reference-resolution error; column/row/sheet references introduce RefN(AnyScalar).
Unknown scoped names conservatively introduce top and unknown external characteristics.
Arrays join scalarized element kinds. Numeric unary/binary operations materialize
references, coerce to their result kind, include errors, and allow array lifting only
when an operand admits arrays or RefN. `@` consumes those forms and returns a scalar.
Reference operators preserve reference possibilities and include reference errors.

Every registry declaration supplies an explicit typed result rule. Scalar consumers
such as SUM discard argument array forms. Matrix producers introduce arrays. Information
predicates lift over arrays. IF, IFS, CHOOSE and SWITCH join possible selected result
forms and also allow lifting from array condition/index arguments. Fallback selection
retains input/fallback forms and lifting. INDEX retains reference versus value channel
possibilities. Some rules intentionally retain any scalar kind, or top for a more
complex result contract; this is conservative precision, not a runtime conversion.
Extensions join their permitted created output forms with possible forwarded argument
forms. Unresolved function calls retain their #NAME? result and opaque-provider observation.
All possible argument characteristics are joined even for lazy calls: static possibility
is distinct from which branch executes.

Possible host-read categories describe direct expression/handler capabilities.
Reference/name characteristics include a source indirection; effects inside referenced
formula bodies are described by their own typing judgments and linked by the existing
cell dependency graph. `HOST_POLICY` covers execution/coercion/text/number/format policy
reads. Context coercion capabilities can overapproximate actual clock/localization use.
The actual dependency trace remains the authority for retained cache validity. The
compiler does not invalidate a cell merely because its possible type includes TIME.

The soundness contract is: if evaluation of an expression succeeds under the bound
contracts, its expression-channel value belongs to γ(T); its observed direct reads and
external behavior are admitted by E (including source indirection). Root materialization
is a separate operation: scalar intersection selects/coerces a scalar from a reference
or array; dynamic spill materializes a root array or multi-reference into anchor/children.
A returned public cell scalar must be checked against that materialization, not directly
against an expression's Ref1/RefN component. `may_spill` concerns that root materialization.
Tests check lattice laws, registry coverage, representative expression-channel conformance,
root policy results, joins and context reuse. They are executable evidence, not a machine
proof for every function implementation.

NOW/TODAY reuse their result when the captured clock and actually consumed conversion
fields are unchanged. Captured random streams reuse while seed, source and draw identity
remain unchanged. Changing consumed context fields invalidates actual consumers and
transitive dependents. Unselected branches add no observed dependency. Explicit volatile
or opaque providers still refresh. Conservative admission-time context events have been
removed: actual coercion/handler boundaries emit context reads instead. Serialized compiler
metadata validates expression boundaries and scalar-kind bits; the Overlay snapshot and
builtin contract versions change with this representation.

The postfix spill-range rule is `Γ ⊢ e# : RefN(AnyScalar) ∨ Scalar(Error)` with
`e`'s effects joined with reference reads. The reference can contain one cell.
Resolving `#` reads and finalizes a single anchor, then uses its accepted footprint;
it preserves reference geometry even when the anchor's stored member is an error.
The stored `ANCHORARRAY` spelling uses the same reference adapter and transfer rule.
Profile-independent compilation also admits every runtime-resolvable Excel profile
handler's catalog result, joined with the unresolved-name error alternative. A
profile-only handler cannot be classified as an error-only scalar before execution.

Dynamic root materialization retains the existing cell-versus-area reference
provenance. A cell reference remains scalar; a single rectangular area reference
materializes through the array path, including a one-cell spill reference. Explicit
`@` still selects a scalar. No new runtime value form or effect tag is needed.

`ProducerPolicy::TrustSaved` restores caller-provided worksheet results rather than
evaluating expressions. Compiled expression types do not validate saved scalar
kinds or freshness: restoration proves geometry and ownership only. Consumers
inspect actual cached values. Typed cache eligibility must not infer a trusted
saved value's kind from the producer expression's possible result type.

## Possible spill geometry

`ShapeBounds` adds independent row and column upper bounds to each expression
judgment: `Γ ⊢ e : (T, D) with E`. A finite bound is a positive integer;
`None` represents infinity (no proof). The geometric concretization is the set
of nonempty rectangles whose height and width do not exceed their respective
bounds. Scalar and single-cell results count as 1 by 1. The product judgment
admits values belonging to the core result concretization whose materialized
rectangular geometry also satisfies D. Nonrectangular reference outcomes that
cannot be materialized remain governed by their existing error/scalar behavior.

Bounds order by numeric ≤ with infinity greatest, independently per axis.
Join takes each axis maximum. Thus `(3,1) ∨ (1,4) = (3,4)` and
`(∞,1) ∨ (2,3) = (∞,3)`. The smallest shape bound is `(1,1)`; impossibility
is represented by the existing core result bottom, not a zero-sized shape.
Serialized zero bounds are rejected. Bounds are upper limits, not promises that
a formula always returns an array, or that it has exactly those dimensions.

Scalar consumers and explicit intersection produce `(1,1)`. Rectangular array
literals retain syntax dimensions. Physical rectangular references retain endpoint
distances; whole rows and columns use grid limits. Computed reference endpoints,
reference unions/intersections, names, spill references, general matrix producers,
INDEX and extension-created or forwarded results remain conservative when no
finite proof exists. TRANSPOSE exchanges axes. Numeric operators and elementwise
selection/lifting join operand shapes per axis: broadcasting never exceeds those
maxima, and incompatible cases retain their existing errors. Core scalar-only
results discard argument geometry. These transfers share the existing registry
result catalog; there is no second function-name table.

SEQUENCE's registry rule proves each dimension separately from a positive finite
numeric literal, with omitted/missing dimensions defaulting to one. Positive
fractional literals use the same truncation as the handler. Other expressions are
unknown on that axis; no symbolic evaluation, random draw or previous result is
used as a proof. Consequently SEQUENCE(3) has `(3,1)`, SEQUENCE(A1,2) has `(∞,2)`,
and TRANSPOSE(SEQUENCE(A1,2)) has `(2,∞)`. Syntax errors have no successful spill;
preparation resource failures cannot provide an exclusion proof.

A source-owned candidate exists only if the core result may spill (or preparation
failed without proving it cannot). Its possible rectangle starts at the formula
coordinate and ends at anchor + bound - 1, clipped to the worksheet grid;
infinite axes extend to that grid edge. Inclusive rectangle intersection includes
sheet identity and checks both axes independently. This rectangle does not reserve
cells. Actual result publication still validates dimensions, authored blockers,
ownership and resource limits atomically. Demand preparation selects intersecting
rectangles and the necessary earlier competitors. Actual footprints settle under
strict row/column priority before ordinary consumers or reusable indexes use them;
possible bounds alone establish neither ownership nor a cycle.

Caller-trusted saved results have a different geometry contract: their declared
extent may exceed the computed type bound. Candidate geometry unions that extent
even when incomplete, so a later read cannot hide a restoration failure. Legacy
undeclared owned children make the bound unknown. This is conservative even when
a source override will later make the saved geometry inapplicable; actual restoration
still validates effective source, content/style intent and completeness. Retained
actual ownership is not a substitute for a future upper bound.

Exact source, parser policy and registry identity protect these compiled proofs.
Lexical template rebinding refuses spillable range-endpoint changes, so changing
A1:A3 into A1:A30 cannot silently retain the old shape. Compiled plan version 4,
Overlay revision 55 and builtin contract 4 invalidate older representations.
The compiled Plan remains a trusted, self-generated local cache: restore checks
expression coverage, nonzero bound encoding and scalar-only geometry consistency,
not a semantic re-proof of arbitrary edited array metadata. Source cells have no
authoritative user-editable annotations. Runtime values gain no duplicate type tags.

SEQUENCE is available in the modern ExcelDocumentation profile; the pinned POI
inventory is unchanged. Its documented row-major ordering and defaults follow
[Microsoft's SEQUENCE reference](https://support.microsoft.com/en-us/excel/functions/sequence-function).
The kernel and tests are independently written. The reference does not settle all
coercion/error details: Poirot currently truncates positive fractional dimensions,
rejects zero/negative/nonfinite/out-of-grid dimensions after numeric coercion with
#VALUE! (earlier coercion errors propagate), and reports
nonfinite generated arithmetic with #NUM!. These are explicit Poirot contracts,
not claims of verified Excel parity. Collection/work limits remain operational
failures before allocation and cannot be caught as spreadsheet errors.

## Scalar classification proofs

Producer discovery asks a narrower question than execution compilation: can this
source return a materialized area? The compiler learns a negative answer only
from a successfully bound formula whose formal root type cannot spill. Within the
same parser policy and registry namespace, that proof covers sources with the
same classified token structure, changed valid point coordinates, and changed
numeric or text literal values of the same core kind. Complete encoded key equality
protects reuse; hash equality alone does not. Function names, qualifications,
operators, whitespace, argument positions and arity remain exact.

This follows the core transfer rules above: result inference uses literal kinds,
not literal values or reference coordinates. Numeric constants influence only
array dimensions, and a scalar-only root discards those dimensions. Conditional
rules join all possible branches; they do not choose a branch from a literal value.
Extensions and runtime-resolvable profile handlers use the same existing catalog
and binding contracts. A scalar consumer can therefore hide changing array
geometry in its arguments without becoming an array producer itself.

The existing classified lexer validates numeric finiteness and escaped text.
Point coordinates are checked against the policy grid. Whole-axis pairs use the
parser's primitive decoder and retain their complete spelling, so their bounds
cannot vary inside a proof class. Token structure preserves expression-count and
nesting limits; parser policy has no source-length or text-length limit. Unsupported
syntax and failed parsing/binding never establish a proof. Changed array-producing
sources still undergo full compilation and fresh geometry inference.

A proof excludes a producer; it does not create an executable, evaluated value,
or authoritative public cell annotation. The actual source is compiled with its
own bindings when execution reaches it, including operand allocation and its
possible operational failure. Learned scalar keys are compiler-owned derived
metadata and are not serialized. Restored compilers safely relearn them. Ordinary
execution-template normalization continues to preserve literal values and complete
whole-axis endpoints; these scalar equivalences cannot rebind execution operands.

Each policy pool may also keep one bounded last-used scalar template. It compares
unchanged literal spans directly and decodes only the varied point cells through
the parser's shared canonical decoder. Literal values, qualifications and whole-axis
endpoints remain exact, so this fast match is a strict subset of the scalar proof
class above. A mismatch uses the complete classification path. Two consecutive
literal-preserving template hints are required before replacing the matcher;
plain alternation and unique literal values do not rebuild it on every query.
The hints govern promotion only: exact spans and validated cells authorize a hit.
Storage admission is fallible and limited to 16 KiB of source and 64 point slots.
The classification pass also determines eligibility and retains bounded point spans,
so promotion does not decode the source again. It reuses the matcher's source and
span buffers, reserving both before changing either value; storage refusal preserves
the old matcher. Declined patterns avoid repeated promotion scans and allocation
attempts. The matcher is derived optimization state and is never serialized.

## Progressive source completeness

Discovery records the exact union of inspected origin prefixes per worksheet. Its
maximal endpoints form a staircase: rows increase and columns decrease. Expanding
a prefix visits only previously uninspected rectangular strips. Candidate geometry
and new coverage publish together after every scan and allocation succeeds; failed
expansion preserves the previous inspection proof.

For demand ending at `(R,C)`, all direct possible owners have anchors within the
origin prefix ending there. For each selected producer anchored at `(r,c)` with
possible right edge `X`, every earlier competitor has row at most `r` and column
at most `X`. Discovery therefore also inspects the prefix ending at `(r,X)` and
repeats selection for intersecting earlier producers. Unknown width uses the grid's
right edge. Unknown height does not enlarge this priority bound. Dependencies add
their own demands, including across worksheets.

At the resulting fixed point, unseen anchors cannot change ownership of this
settled demand. Later discovery can safely expand the same staircase. Source
inspection alone does not establish actual ownership: cached range/index reads
also require all direct possible owners to have settled. Neither an empty partial
candidate index nor an unfinished strip proves that a range is ready. The derived
frontier follows the existing source/policy invalidation contract and is relearned
after snapshot restoration.

The staircase is keyed by column, with strictly decreasing row endpoints. The
first endpoint at or right of a demand supplies its maximum covered row in
logarithmic time. Expansion visits smaller columns in reverse, emitting new row
bands. Every visited endpoint except at most one retained boundary is dominated
and removed on successful commit. This avoids rescanning or copying a long
frontier for sparse, mutually incomparable demands. Only new strips and replaced
column keys are staged; the frontier is not copied during expansion.

The ordered candidate list retains a monotone prefix maximum of possible ending
rows. A query excludes prefixes that end above its first row and suffixes whose
anchors cannot precede its last row or priority limit. Exact rectangle intersection
still decides membership. Unknown or tall rectangles can defeat this pruning; it
does not narrow formal possible geometry. Candidate and row-summary storage is
reserved before publishing source coverage, and interleaved insertions rebuild
the summary after sorting.

A `SettledArea` is a separate runtime fact: discovery and actual ownership
settlement both completed for that range in this evaluation cohort. One such fact
can serve contained member reads and index eligibility. Successful range resolution
seals it only outside private settlement; point reads do not replace it. Private
attempts and sticky failure disable it. Cohort reset clears it, and snapshots do
not serialize it. This fact is stronger than inspected source coverage and is not
an additional formula type or a promise that unknown producers have finite shape.
