# poirot-workbook

Compose workbook data, sparse overlays, evaluation instances and file adapters.
`Workbook` owns source data. `Overlay` owns input edits, lazy compilation and a
persistent dependency-aware result cache. `OverlayEvaluation` borrows the overlay
with per-run settings from `EvaluationContext`. Direct `Evaluation` owns temporary
edits and local results, clears results on edits, and keeps no dependency graph.
Both use the same calculation and spill engine.

```rust
use poirot_workbook::{Workbook, Overlay, EvaluationContext, model::CellValue};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut wb = Workbook::new();
    wb.add_sheet("S")?.set_formula("B1".parse()?, "A1+1", CellValue::Blank)?;
    let context = EvaluationContext::default();
    let changes = {
        let mut overlay = Overlay::with_workbook(&wb).build()?;
        let input = overlay.target("S!A1")?;
        let output = overlay.target("S!B1")?;
        overlay.set_value(input, 4.0)?;
        assert_eq!(overlay.evaluation(&context)?.cell(output)?, CellValue::Number(5.0));
        overlay.changes()
    };
    wb.apply(changes)?;
    Ok(())
}
```

`outputs(&targets)` evaluates a list; `all()` evaluates all stored values and
materialized spill children. Repeated calls on one instance share budgets and
memoization. New instances reuse valid overlay results, refresh volatile cells,
and invalidate consumers of changed context fields. Source writes invalidate
transitive point/range dependents, including ranges containing previously absent
cells. Spill ownership is retained as a complete footprint, not isolated scalars.

`Workbook::snapshot()` and `Overlay::snapshot()` return borrowed Serde views.
Deserialize into `WorkbookSnapshot<'static>` or `OverlaySnapshot<'static>` and
restore with `from_snapshot`. Overlay restoration returns a workbook and detached
`WorkspaceState`. Use `state.attach(&workbook)`, `state.attach(Arc::new(workbook))`,
or `attach_with_extensions` to validate and choose ownership. Direct evaluation
starts with `Evaluation::with_workbook(handle).build(context)`; its snapshots
restore source/bytecode through `state.into_evaluation(handle, context)`, with
fresh results and no graph. Both owning workspaces are `Send` when their handles
and runtime services permit transfer. Overlay snapshots include the source workbook,
overrides, bytecode, result-cache dependencies and spill ownership. Caller-owned
codecs, storage and input limits remain separate. Use trusted same-engine data.

`Spreadsheet` retains file-import warnings and exposes its `Workbook` through
`workbook()`/`workbook_mut()`. Saving is explicit through `SavePlan`; imports and
exports do not calculate. Formula-aware structural edits remain in `edit` and
`translation_edit`. The [root guide](../../README.md) and
[evaluation guide](../../docs/evaluation.md) describe the full workflow.
