# Workbook, overlay and evaluation

`Workbook` is the sparse authoring model. `Overlay::with_workbook(&wb).build()`
borrows it immutably and allocates no copy of upstream cells. Supply an
`Arc<Workbook>` handle for shared baseline ownership across worker threads. An overlay stores edited cells, tombstones, reached
compiled programs, computed values and dependency metadata. Cached-only entries
retain no duplicate source formula or style.

Resolve sheet-qualified cells with `overlay.target("'My Sheet'!A1")`, or construct
an `EvaluationTarget` using a zero-based sheet index and `CellAddress`. Setters
validate coordinates. `set_value` replaces a formula, `set_formula` normalizes
formula source and clears its saved value, `set_style` edits presentation, and
`remove_cell` hides the upstream cell. Returning a cell to its upstream state
removes its override. `reset()` drops edits and evaluated state but keeps immutable
compiled programs. `clear_cache()` drops only evaluated state.

## Ownership and evaluation

`OverlayPolicy` contains parser, numeric, builtin, text, array and materialization
policies. `with_policy` configures the builder; `set_policy` invalidates affected
artifacts. A materialization-only change preserves raw computation. Text policy
changes invalidate actual text-policy consumers. Parser/computation changes
invalidate the appropriate compilation/results.

`EvaluationContext` contains per-run clock, seed, localization, formatting inputs,
budgets and retention bounds. It owns no edits or cache. Create a fresh instance
with `overlay.evaluation(&context)`, returning `OverlayEvaluation`:

- `cell(target)` returns a single `CellValue`.
- `outputs(&targets)` returns `CellEvaluation` records in caller order, including duplicates.
- `all()` returns effective stored values and materialized spill children, in coordinate order.

`all()` does not expand the worksheet's implicit blank coordinates.

### Time and reproducibility

`as_of_time` is a captured `std::time::SystemTime`, shared unchanged by both
evaluation paths and extension providers. To capture the current time:

```rust
let context = poirot::EvaluationContext {
    as_of_time: std::time::SystemTime::now(),
    ..Default::default()
};
```

Reuse that context to freeze time across runs. Cache identity and snapshots retain
nanosecond precision, including times before the Unix epoch. Only date and coercion
operations convert the instant into numeric seconds; localization supplies the
separate UTC offset. Random streams are seeded and reproducible; reexecuting RAND
with the same seed may return the same value. Overlay evaluations invalidate
executed volatile and opaque providers and their dependents on each new run;
changed context fields invalidate the formulas that consumed them.

### Results and failures

One instance shares execution memoization and budgets across calls. Overlay edits
require ending its borrow first. A successful call commits its whole requested
batch. A failure commits no new values from that batch, poisons the instance and
leaves earlier completed calls intact. Create another evaluation to retry.
`values()` inspects reached run-local values, and `usage()` distinguishes formula
executions, session memo hits and persistent cache hits.

`CellValue::Error` is a spreadsheet result; `EvaluationFailure` is an operational
abort that formula error handlers cannot intercept. Finite default budgets can
refuse large requests. Literal/file-saved values never authorize cached reuse.

## Direct evaluation

`Evaluation::with_workbook(handle).with_policy(policy).build(context)` creates
an editable workspace without retained dependency tracking. It shares source,
compilation, reference/spill execution, output projection, and guarded change
export with overlay evaluation. The same `set_value`, `set_formula`, `set_style`,
`set_cached_value`, `remove_cell`, `cell`, `outputs`, and `all` operations apply.
Each effective source edit clears all local values, derived spill ownership, and
lookup prefixes. Bytecode and cumulative work budgets survive. `clear_cache()`
does not recover a failed run; `begin(context)` explicitly resets run state,
budgets and failure while preserving source edits and bytecode. `reset()` also
removes edits but does not implicitly create a new evaluation identity.

Both direct Evaluation and Overlay accept `Borrow<Workbook>` handles and are
`Send` when their fields permit transfer. Shared `Arc` baselines need
`Workbook: Send + Sync`. Borrowed handles can cross scoped threads. Each worker
owns its mutable workspace; the cache itself needs no locking. Keep baseline
source stable while attached.

Runtime providers can be borrowed with `with_extensions(&registry)` or shared with
`with_shared_extensions(Arc<ExtensionRegistry>)`. The shared form lets already-used
workspaces move into ordinary spawned threads. Snapshot attachment offers the
corresponding borrowed/shared extension methods; implementations remain outside Serde.

## Compilation, snapshots and changes

Preparation is lazy and owned by the overlay. `compile_all()` optionally prepares
cell and defined-name formulas. Immutable bytecode templates are shared while
reference bindings remain source-specific. Spill bounds are computed during
compilation so discovery does not require retaining/reparsing the AST.
`compiled_usage()` (requiring mutable workspace access) and `evaluation.usage()` distinguish preparation work,
executed formulas and cache hits.

For stored files, `poirot::artifact` owns a versioned envelope, bounded decoding,
source recovery and explicit prepared-state disposition. See [snapshot files](snapshot-files.md)
for compatibility policy and the complete on-disk example. Raw views remain available
for callers with an existing application-owned encoding contract.

Both source workbooks and overlays provide `snapshot()` borrowed Serde views.
Runtime owners do not implement Serde. Deserialize the appropriate snapshot as
`'static`, then call `from_snapshot`; `into_owned` also detaches an existing view.
An overlay snapshot includes its base workbook, sparse writes/removals, shared
bytecode, result values, consumed dependencies and spill metadata. Numeric bits
and exact UTF-16 survive serialization. Runtime providers are excluded and must
be reattached with `WorkspaceState::attach_with_extensions`. Overlay restoration
returns `(Workbook, WorkspaceState)`; `state.attach(handle)` compares the complete
baseline, including literal values and exact numeric bits, before accepting it.
Detached state temporarily keeps source identity records until attachment; live
workspaces retain only a baseline handle and their sparse diff. Direct snapshots
omit untracked results and attach through `into_evaluation(handle, context)` (or
the extension-aware counterpart), starting a fresh run with restored bytecode. Use trusted same-engine data,
an application-owned identity envelope, and decoder resource limits.

`Overlay::changes()` exports owned, `'static` `WorkbookChange` values containing
source edits and retained result writes. `Evaluation::changes()` exports authored
edits and successful result writes together. Both have consuming `into_changes()`
methods. `OverlayEvaluation::result_changes()` exports only completed result
writes from that borrowed evaluation; `into_result_changes()` also ends its borrow.
Both return `Result` and report conflicting internal borrows as `EvaluationFailure::StalePlan`.
`Overlay::changes()` and `Overlay::cached_cells()` require mutable access, allowing
exclusive cache inspection without a runtime borrow that can panic.
Export full workspace changes when publishing authored edits together with results.
`SetValue` replaces a formula; `SetCachedValue` checks the expected formula and
preserves it. Spill children have explicit cache
writes and ownership; clearing a footprint restores the prior authored cell.
`wb.apply(changes)` validates and applies an entire batch atomically.
`WorkbookChange` also supports Serde serialization.

## Files and source metadata

`Spreadsheet::open`, `from_bytes` and `load_bytes` retain import warnings.
Evaluate an overlay over `workbook()`, then apply changes to `workbook_mut()`.
`plan_xlsx_save` and `plan_xls_save` validate borrowed source and serialize without
recalculation. Unsupported fidelity remains explicit. Snapshots are separate from
XLS/XLSX encoding; not all file formats retain every runtime feature.

Sheet/name/row topology belongs to the workbook. End the overlay borrow before
applying structural workbook changes. `edit::EditPlanner` and
`translation_edit::FormulaTranslationEditPlan` provide formula-aware structural
transactions. Low-level compiler, execution ports and provenance tools in the
subsystem crates support engine integration; ordinary callers need no planner,
execution plan, calculator, or separate runner.
