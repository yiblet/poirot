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
P ::= T with E
external behavior ::= Tracked ≤ Volatile ≤ Opaque
```

`FormulaType` stores the union in five components. Empty scalar-kind sets mean an
absent alternative. Bottom has no alternatives. Top has every scalar kind in each
form plus Missing. `Ref1` describes a single coordinate. `RefN` allows one or more
coordinates, possibly multiple areas or sheets. A single-cell source reference is
therefore not spillable merely because its expression returns a reference. Shapes,
coordinates and stored values remain runtime data.

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
