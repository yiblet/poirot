# Calculate and export scenarios

Use direct evaluation when you need selected outputs and do not need to retain unaffected results after edits. The workbook remains unchanged:

```rust
use poirot::{CellValue, Workbook};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut book = Workbook::new();
let sheet = book.add_sheet("Energy")?;
sheet.set_value("A1".parse()?, 12.)?;
sheet.set_formula("B1".parse()?, "A1*25")?;
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

Resolve string targets once for repeated reads. Resolution accepts quoted sheet names such as `'Annual summary'!B12`. A missing sheet is an error; a valid unstored coordinate calculates as blank. Targets bind to the source workbook's sheet topology and are not guaranteed to survive structural edits. Worksheet errors such as `#DIV/0!` are calculated values; operational refusals are Rust errors.

Use `book.stored_cell(sheet, address)` to inspect authored formulas, style and saved values without calculation. Saved values may be stale. Like the compatibility method `book.cell`, this returns `None` for either a missing sheet or an unstored cell. Resolve `book.sheet_by_name(name)` first when you need to distinguish those cases.

## Retain unaffected results with an overlay

```rust
# use poirot::{Workbook, EvaluationContext};
# fn main() -> Result<(), Box<dyn std::error::Error>> {
# let mut book = Workbook::new();
# book.add_sheet("Energy")?.set_formula("B1".parse()?, "A1*25")?;
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

An overlay also stops propagating changes when an intermediate result remains identical. For example, changing an input from `1.1` to `1.2` reruns `ROUND(input,0)` but can reuse formulas that depend on its unchanged result. Reuse follows the dependencies actually consumed, including lazy branches and range membership. Dirty dependencies are validated in their original consumption order. If they all retain their previous results, the parent stays cached; the first changed result resumes ordinary formula execution so an obsolete lazy branch is not evaluated. Replacing a formula forces it to execute; style edits conservatively force direct readers. Volatile, opaque and changed-context dependencies retain their own invalidation rules.

Under dynamic spilling, an eligible blank coordinate retains dependencies on possible incoming spill owners, including an empty spatial query. Adding or removing an anchor invalidates matching coordinate reads. Value-only edits can reuse an unchanged producer and its complete array footprint; array shape and ownership are part of validation. Authored formulas and nonblank values block incoming spills. Source metadata changes and sparse range consumers still invalidate conservatively, and child reads restore the complete footprint before exposing retained spill values.

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
| `apply_edits_to_workbook()` on an owned or exclusively borrowed advanced overlay | Applied, no longer pending | Kept in overlay | Authored changes applied atomically |
| `save_xlsx()` | Kept | Kept | Unchanged |

Applying edits does not write calculated formula results into the baseline or save a file. Shared immutable baselines cannot be mutated through this operation. Clearing results or discarding edits preserves direct cycle work and any prior operational failure; only `start_cycle` resets them. Legacy `clear_cache`, `reset`, `begin` and `flush` keep their existing meanings.

## Restore native state

```rust
# use poirot::{Workbook, EvaluationContext};
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

Use `.prepared_policy(artifact::PreparedPolicy::RequireCompatible)` to require present, compatible preparation (refusing source-only snapshots as well), or `SourceOnly` to discard it deliberately. The advanced `Workbook::load_snapshot()` retains its historical compatibility-if-present strict policy. `into_artifact()` and that loader retain the advanced APIs, including incremental checkpoint attachment and custom provider management.

## Existing names and lower-level ownership

`EvaluationPolicy` now names the spreadsheet policy used by both direct and incremental workflows. `OverlayPolicy` remains its compatibility spelling. The distinct compiler policy keeps `eval::EvaluationPolicy`; `PreparationPolicy` explicitly names that concept at the crate root. `NumberLocale` aliases `NumberLocaleV2` for numeric input parsing; display formatting has separate settings.

The workbook builders borrow their source and do not clone it. Existing `Evaluation::with_workbook`, `Overlay::with_workbook` and ordinary `Calculation` APIs still support model workbooks, owned workbooks and shared handles. They retain their existing reporting contracts. `WorkbookEvaluation::from_calculation` and `WorkbookOverlay::from_overlay` add file export to those ownership forms without copying or resetting their state.
