# Calculate and export scenarios

`Workbook` owns the editable source: cells, formula text, saved results and styles.
`Evaluation` and `Overlay` keep scenario inputs and live calculation results beside
that source. A computed `CellValue` is not automatically written into a stored
`Cell`. Bulk output calls return `CellEvaluation` records carrying each target,
value and optional source formatting. The [architecture guide](architecture.md#from-stored-cells-to-calculated-outputs)
connects these types to sparse worksheet storage.

| Workspace | Lifetime of calculated results | Entry point |
| --- | --- | --- |
| `Evaluation` | Reused within a direct cycle; edits or a new cycle clear them | `book.evaluation().build()` |
| `Overlay` | Validated and reused across edits and cycles using dependency evidence | `book.overlay().build()` |

Both builders borrow the workbook without copying it. Their workspaces keep edits
local, and exports capture effective state. `book.recalculate()` instead writes
calculated results into the source workbook.

Use direct evaluation when you need selected outputs and do not need to retain unaffected results after edits. The workbook remains unchanged:

```rust
use poirot::{CellValue, Workbook};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut book = Workbook::new();
let sheet = book.add_sheet("Energy")?;
sheet.set_value("A1", 12.)?;
sheet.set_formula("B1", "A1*25")?;
let mut evaluation = book.evaluation().build()?;
let hours = evaluation.target("Energy!A1")?;
let energy = evaluation.target("Energy!B1")?;
evaluation.set_value(hours, 24.)?;
assert_eq!(evaluation.cell(energy)?, CellValue::Number(600.));
let bytes = evaluation.save_xlsx().to_buffer()?;
# Ok(())
# }
```

The builder captures one default context when built. `.context(context)` replaces clock, seed, locale and limits; a later `.limits(limits)` overrides only quotas. `.policy(policy)` selects shared spreadsheet semantics. `.profiling(true)` enables detailed reports, available through `finish()`; otherwise report counters are `None`. No cell is evaluated during construction.

Resolve string targets once for repeated reads. Resolution accepts quoted sheet names such as `'Annual summary'!B12`. A missing sheet is an error; an unstored coordinate on an existing sheet is a valid target. It calculates as blank when no source, scenario edit or spill supplies a value. Targets bind to the source workbook's sheet topology and are not guaranteed to survive structural edits. Worksheet errors such as `#DIV/0!` are calculated values; operational refusals are Rust errors.

Use `book.stored_cell(sheet, address)` to inspect authored formulas, style and saved values without calculation. Saved values may be stale. It accepts A1 text, a typed address, or zero-based `(row, column)` integers. Invalid coordinates return an error; `Ok(None)` means either a missing sheet or an unstored cell. Resolve `book.sheet_by_name(name)` first when you need to distinguish those cases.

## Retain unaffected results with an overlay

```rust
# use poirot::{Workbook};
use poirot::calculation::EvaluationContext;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let mut book = Workbook::new();
# book.add_sheet("Energy")?.set_formula("B1", "A1*25")?;
let mut overlay = book.overlay().build()?;
let hours = overlay.target("Energy!A1")?;
let energy = overlay.target("Energy!B1")?;
let context = EvaluationContext::default();
for value in [12., 24.] {
    overlay.set_value(hours, value)?;
    overlay.evaluation(&context)?.cell(energy)?;
}
let bytes = overlay.save_xlsx().to_buffer()?;
# Ok(())
# }
```

Each `evaluation(&context)` starts an explicit cycle and resets its work allowance. Edits and valid results survive between cycles. Use `evaluation_with_profiling(&context, true)` when you need category counters. Direct reads instead share the cycle started by construction or `start_cycle(context)`. Editing does not replenish work. Use `book.recalculate()` when you want a full calculation written into the workbook's stored results.

Group scenario inputs with `overlay.edit` to invalidate their combined dependencies once:

```rust
# use poirot::{Workbook, ModelError};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let mut book = Workbook::new();
# book.add_sheet("Energy")?;
# let mut overlay = book.overlay().build()?;
let hours = overlay.target("Energy!A1")?;
let price = overlay.target("Energy!A2")?;
overlay.edit(|edits| -> Result<(), ModelError> {
    edits.set_value(hours, 24.)?;
    edits.set_value(price, 0.15)?;
    Ok(())
})?;
# Ok(())
# }
```

The closure exposes only edits; reads and evaluation resume after it returns. Edits apply in order. If an operation fails or the closure unwinds, earlier successful edits remain applied and their results are invalidated. This groups invalidation without rollback or a separate commit step.

An overlay can reuse downstream formulas when a changed input produces the same
intermediate result. Reuse follows consumed dependencies, including lazy branches,
ranges and the execution context. Replacing formulas and changing styles can
invalidate readers even when a numeric result is unchanged. The
[architecture guide](architecture.md#reuse-and-edits) explains the engine's
dependency validation.

Array arguments follow the selected spreadsheet policy. Shared kernels broadcast
singleton axes: `ROUND(A1:A3,{0,1})` can spill a three-row, two-column result;
collection consumers such as `SUM` consume whole operands. Errors can belong to
individual array members. Some manual function adapters retain policy-specific
behavior; see the [compatibility guide](compatibility.md).

An explicit `@A1:A3` selects the source cell in the formula's row.
`ArrayResultPolicy::ScalarIntersection` projects only the final result;
intermediate arrays remain available to consumers such as `SUM(ROUND(A1:A3,1))`.
Authored formulas and nonblank values block incoming spills. Overlay reuse tracks
array shape and ownership as well as values, including changes to possible spill
anchors and previously empty coordinates.

After an edit, dirty results remain private evidence until a successful output call validates or replaces them. `cached_value()` and `cached_cells()` expose only current results. `retained_usage()` includes dirty storage as well as current values, so storage limits still apply before recalculation. Export and native snapshots omit dirty evidence. A failed output batch publishes none of its new results or validations.

## Export is a snapshot of effective state

`save_xlsx()` captures effective authored edits and currently valid results in one isolated workbook copy. It never evaluates formulas, applies edits to the baseline, or clears warm state. The returned builder owns that captured copy: later scenario edits do not change a pending export. `.options(options)` uses the same output settings and defaults as a workbook save. Validation and serialization finish before a destination file is opened; filesystem failure can still leave a partial file.

Export follows the existing `Workbook` file compatibility contract, including spill ownership and cleanup and supported XLSX features. Live source metadata and import warnings remain available; this does not promise that diagnostics themselves are serialized into XLSX. Untouched saved formula values are preserved and may be stale. Calculating one output does not make every saved result current. Call `all()` explicitly before export when you need a full calculation. Native snapshots preserve engine preparation and remain distinct from XLSX files.

## State changes

| Operation | Authored edits | Calculated results | Baseline |
| --- | --- | --- | --- |
| `clear_calculated_cache()` | Kept | Cleared | Unchanged |
| `discard_edits()` | Discarded | Cleared | Unchanged |
| Direct `start_cycle(context)` | Kept | Cleared; work and failure reset | Unchanged |
| Overlay `evaluation(&context)` | Kept | Valid results reused; work reset | Unchanged |
| `apply_edits_to_workbook()` on an owned or exclusively borrowed overlay | Applied, no longer pending | Kept in overlay | Authored changes applied atomically |
| `save_xlsx()` | Kept | Kept | Unchanged |

Applying edits does not write calculated formula results into the baseline or save a file. Shared immutable baselines cannot be mutated through this operation. Clearing results or discarding edits preserves direct cycle work and any prior operational failure; only `start_cycle` resets them. The lower-level `poirot::eval::overlay` APIs expose engine lifecycle operations directly.

## Restore native state

```no_run
# use poirot::Workbook;
use poirot::calculation::EvaluationContext;
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let restored = Workbook::restore_snapshot().open("model.poirot")?;
let mut restored = restored.into_evaluation(EvaluationContext::default())?;
println!("Preparation: {:?}", restored.prepared_status);
let output = restored.evaluation.target("Summary!B12")?;
let value = restored.evaluation.cell(output)?;
# Ok(())
# }
```

`prepared_status` distinguishes reused preparation, absent preparation, incompatible preparation discarded during recovery, and explicit source-only recovery. Compatible saved spreadsheet policies take precedence. When preparation is absent or discarded, default `EvaluationPolicy` semantics apply; `into_evaluation_with_policy(context, fallback)` overrides only that recovery fallback. The context always starts a new direct cycle; direct result caches are not restored as warm incremental state.

Use `.prepared_policy(poirot::snapshots::PreparedPolicy::RequireCompatible)` to require present, compatible preparation (refusing source-only snapshots as well), or `SourceOnly` to discard it deliberately. Component decoding enforces the same strict requirement. `restored.into_overlay()` retains compatible incremental results; `restored.into_workbook()` recovers authored source including edits, without promoting calculated results. Attach runtime providers with `Workbook::restore_snapshot().shared_extensions(registry)` before restoring custom-function preparation. Provider identities must match exactly; source-only recovery binds against the newly attached registry. Checkpoints store the workbook once; preparation saved by an earlier engine revision is discarded (or refused under `RequireCompatible`) while its source still restores.

Prepared incremental checkpoints retain calculated results and bounded exact-lookup search evidence. Attachment rebuilds the previously inspected spill-discovery regions from authored source without evaluating formulas. This work belongs to restoration; subsequent cycles retain the same discovery and lookup reuse as an in-memory overlay. New source edits, execution-context changes and resource admission still invalidate or discard affected state normally.

## Public vocabulary

`Evaluation` is the direct scenario returned by `Workbook::evaluation().build()`;
`Overlay` is the incremental scenario returned by `Workbook::overlay().build()`.
Their builders are `EvaluationBuilder` and `OverlayBuilder`. Both expose explicit
methods for editing, calculating, inspecting state and exporting. Neither forwards
arbitrary engine methods through `Deref`. `EvaluationCycle` borrows an overlay for
one cycle, and `EvaluationReport` represents completion with optional profiling.

`EvaluationPolicy` selects the spreadsheet semantics shared by both modes.
`PreparationPolicy` selects compiler preparation; it is a different concept.
`NumberLocale` controls numeric input parsing; display formatting is separate.

Use `Evaluation::with_workbook(source)` or `Overlay::with_workbook(source)` for a
borrowed, owned or shared model, including `Arc<poirot::Workbook>`. Both
builders accept `.extensions(&registry)` and `.shared_extensions(Arc::clone(&registry))`;
registration types live in `poirot::calculation::extensions`. `Evaluation::set_style`
changes scenario presentation and clears results while preserving work and failure,
just like value edits.

Advanced integrations can wrap an already configured engine with
`Evaluation::from_calculation` or `Overlay::from_overlay` without resetting state.
`into_calculation` and `into_overlay` return that engine state. Compiler, observer
and run instrumentation remain available through `poirot::eval`.


Consuming builders (`book.into_evaluation()` and `book.into_overlay()`) move the
model into the engine without copying it. `into_workbook()` recovers that original
baseline and discards pending edits and computed results. On an owned overlay,
call `apply_edits_to_workbook()` first to preserve authored edits; call
`recalculate()` on the recovered workbook when you need stored computed values.
Applying edits preserves warm compiled programs and valid scenario results.
