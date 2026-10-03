# Runtime extensions and pinned ATP aliases

`registry::resolve_function` recognizes `_xlfn.` only for the 67 implemented
registrations frozen from pinned POI `AnalysisToolPak.createFunctionsMap`.
It strips exactly the lowercase prefix, then compares ASCII function names
without case sensitivity. `_XLFN.IFNA` and `_xlfn.SUM` remain unknown.
`_xlfn.PERCENTILE.INC` is not an invented alias. Base implementations still
come from their typed family handlers. RAND and RANDBETWEEN use the calculation-owned seeded random stream;
name registration alone does not establish behavioral compatibility.

`extensions::ExtensionRegistry` is a per-application registry, borrowed
immutably for each execution. `UserDefined` entries use ASCII case-insensitive
lookup, corresponding to POI `DefaultUDFFinder`'s `Locale.ROOT` policy for ASCII.
Unicode function names are rejected explicitly pending a Java-compatible
Unicode casing policy. `AnalysisToolPak` registration accepts exact uppercase
pinned placeholder names only, corresponding to POI's guarded registration
path. Prefix stripping applies to ATP lookup, never to arbitrary UDF names.
Known ATP names require the ATP namespace; UDF lookup cannot shadow the
pinned ATP finder. Builtin overrides and POI's forced ATP override mode are not exposed by this
bounded API; rejected registrations report a typed registration error.

Every provider supplies an implementation signature (including captured
configuration), arity, eager/deferred strategy, cacheability and reference,
array and context capabilities. There is no inferred deterministic contract.
`Send + Sync` permits sharing a provider, but says nothing about result reuse.
Every registration explicitly selects `ExtensionCacheability`:

- `Deterministic`: tracked host reads, arguments, and the implementation signature
  determine its value. Update the signature when code or captured configuration changes.
- `Versioned { external_inputs_revision }`: also reads an immutable external-data
  snapshot identified by the revision. Replace the registration and rebind when
  that snapshot changes. The revision is part of exact snapshot compatibility.
- `Volatile`: execute when reached in every new evaluation, even with unchanged inputs.
- `Opaque`: dependencies cannot be described reliably; conservatively refresh on
  every new evaluation. Ambient state and side-effect counts do not carry a
  deterministic equivalence guarantee.

All providers can be memoized within an evaluation. They must not rely on the
number or order of invocations for correctness. Context-capable providers are
conservatively dependent on every exposed context field. Deterministic and
versioned results can survive overlay snapshot restoration with a matching
registry; incompatible external revisions reject restoration.
Registry registration/replacement also advances an epoch; immutable bound
extension IDs change on replacement. Each registration identity includes its
opaque executable ID: registries built in different insertion orders cannot
claim equal identities while assigning different IDs to the same names. This is explicit identity, not a hash of
an opaque closure address.

`FormulaPlan::bind_with_extensions` binds names once into typed extension
bindings and retains the full structural registry identity. Unknown calls
remain unresolved expressions; errors are deferred to selected branches.
`extensions::run_plan_with_extensions` rejects a stale registry identity.
`run_formula_with_extensions` and `run_cell_with_extensions` also make the
registry available to dependency cell formulas and defined-name formulas.
Existing evaluation wrappers retain their original signatures and behavior.

The callback receives a budgeted `ExtensionHost` backed by the deferred operand
host. It can inspect missing arguments separately from blank cells, request
scalar intersection, inspect shape and read individual array/reference cells.
Reference arguments are not grid-expanded. It can return a scalar, rectangular
array, or an argument preserving its reference/array/missing channel. Output
text, array materialization and nonfinite numbers are validated at the boundary.
Eager providers resolve argument expressions in source order and validate their
kinds before arity handling; reference/array members are still read only on demand.
Errors remain inspectable spreadsheet values. Deferred providers check arity first
and resolve only arguments they request. Operational failures remain separate from
spreadsheet errors, with the invocation cell attached. A provider cooperates
with operation budgets using `charge_work`; opaque Rust execution itself
cannot be instruction-metered by the evaluator.

`ExtensionEvaluation` includes the structural registry identity and an
`ExtensionTrace` of consumed bindings with their declared contracts and cell
locations. Skipped branches contribute no invocation. Downstream compiled
source identity must include `ExtensionIdentity`; recalculation invalidation
must consume signatures/contracts and cacheability, rather than guess from
function names. A volatile call requires reevaluation; context-capable calls
may read position and the evaluation context. Deterministic and versioned calls
retain their tracked host reads; volatile and opaque calls refresh each evaluation.
These APIs do not imply BIFF serialization support.

```rust
use poirot_eval::{extensions::*, registry::Arity};
use poirot_model::CellValue;
let mut registry = ExtensionRegistry::new();
registry.register(
    ExtensionNamespace::UserDefined, "ANSWER", "answer-v1",
    ExtensionContract {
        arity: Arity { minimum: 0, maximum: 0 },
        evaluation: ExtensionEvaluationStrategy::Eager,
        capabilities: ExtensionCapabilities { references: false, arrays: false, context: false },
        cacheability: ExtensionCacheability::Deterministic,
    },
    |_host: &mut dyn ExtensionHost| Ok(ExtensionOutput::Scalar(CellValue::Number(42.))),
)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

The retained `aliases036/prefix-poi` batch uses one HSSF workbook. Its first
cell registers `_xlfn.IFNA`; subsequent case variants reuse that external
name, evaluate to 2 and persist as `_xlfn.IFNA`. `LinkTable` searches external
names without case sensitivity before `InternalWorkbook` consults the UDF
finder. Those observations establish name reuse in that workbook; they do
not by themselves establish case-insensitive prefix stripping in a fresh
workbook or the XSSF parser. Raw formula differences remain retained in the
original comparison. The isolated/reversed-order probe confirms this distinction: fresh exact
lowercase prefixes preserve their suffix spelling and evaluate to 2; fresh
uppercase or mixed prefixes produce native `NotImplementedException`.
A mixed-prefix first entry causes all later variants to reuse its exact spelling
and produce the same native exception. In this reversed case the finder cannot
register an add-in name: `FormulaParser.addName` creates a defined function-name
record, which later calls find through the workbook name lookup before asking
the external-name table. ROI's stable legacy unknown-function
result is `#NAME?`; this is a different failure channel and is not recorded as
POI exception parity.

`external_namespace::ExternalFunctionNamespace::from_stored_names` exposes an
explicit frozen lookup projection for this ordering contract. The supplying
importer/caller must project actual function-name metadata, including defined
function names when applicable, after preserving their source lookup priority;
ordinary defined ranges must not be supplied as function names. The adapter
retains the first
stored spelling for each ASCII-insensitive key, bounds input entries including
duplicates, and returns `Stored`, `Absent` or `UnsupportedName` on lookup.
Names longer than 255 bytes and non-ASCII names require a different installed
policy. `ExtensionRegistry::set_external_namespace` accepts this explicit
metadata and advances the registry epoch; its structural identity includes the
case policy version and exact retained spellings. Binding applies the adapter
once before typed resolution. Unknown stored spellings remain deferred unknown
calls; the adapter does not repair them into known aliases. It never infers
external names by scanning formulas.

Parsed source spelling remains separate from the resolved executable. An
importer can supply the validated function-name projection from BIFF metadata;
record kinds and defined-name precedence remain the importer's responsibility.
Writing the canonical names and retaining the BIFF record kind
belongs to the serialization boundary. Compilation and invalidation consume
the exact registry identity. No new standalone prefix
case normalization is inferred from shared-workbook observations.
