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
