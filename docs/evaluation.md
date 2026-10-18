# Evaluation and overlays

[Documentation](README.md)

Use `Evaluation` for a single calculation or an edit-and-save operation. Use
`Overlay` for repeated what-if calculations that should reuse unaffected results.
Both use the same formula engine and keep edits separate from the baseline.

| Behavior | `Evaluation` | `Overlay` |
| --- | --- | --- |
| Create | `Evaluation::with_workbook(&workbook).build(context)?` | `Overlay::with_workbook(&workbook).build()?` |
| Calculate | `cell`, `outputs`, `all` | Through `overlay.evaluation(&context)?` |
| After an input edit | Clears local calculated results | Invalidates the changed cells and their dependents |
| Retained preparation | Compiled formulas | Compiled formulas, dependencies and valid results |

## Run repeated scenarios

```rust
use poirot::{CellValue, EvaluationContext, Overlay, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Inputs")?
        .set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;
    let context = EvaluationContext::default();
    let mut overlay = Overlay::with_workbook(&workbook).build()?;
    let input = overlay.target("Inputs!A1")?;
    let output = overlay.target("Inputs!B1")?;

    for value in [21.0, 24.0] {
        overlay.set_value(input, value)?;
        let mut evaluation = overlay.evaluation(&context)?;
        println!("{:?}", evaluation.cell(output)?);
    }
    let changes = overlay.into_changes();
    workbook.apply(changes)?;
    Ok(())
}
```

Resolve sheet-qualified references with `target("'My Sheet'!A1")`. Alternatively,
construct `EvaluationTarget` with a zero-based sheet index and `CellAddress`.
An overlay evaluation borrows its workspace; end that borrow before editing it.

## Choose what to calculate

- `cell(target)` returns one typed `CellValue`.
- `outputs(&targets)` returns `CellEvaluation` records in caller order, including
  duplicates. Each has a raw value and optional `CellStyle` formatting.
- `all()` returns effective stored cells and discovered spill children in coordinate
  order. It does not enumerate every implicit blank coordinate in a worksheet.

Display formatting does not round or replace numeric results. Render it
separately as described in [values and formatting](values.md).

## Configure calculation behavior

Use `OverlayPolicy` with the builder's `with_policy` method for formula parsing,
numeric behavior, builtin compatibility, text semantics, array results and saved
cache materialization. The builtin default is `BuiltinSemanticsPolicy::PinnedPoi`.
`ExcelDocumentation` selects supported documented behavior; it does not imply
complete Excel compatibility. See the [builtin contracts](../crates/poirot-eval/docs/excel-builtin-policy.md)
for the differences.

Array results default to scalar intersection. To enable supported dynamic spills:

```rust
let policy = poirot::OverlayPolicy {
    arrays: poirot::eval::ArrayResultPolicy::DynamicSpill,
    ..Default::default()
};
```

Pass the policy to either workspace builder. Spills require supported bounded
geometry and unblocked destination cells; blocked publication produces `#SPILL!`.
See [dynamic array results](../crates/poirot-eval/docs/dynamic-array-results.md)
for supported functions, broadcasting and file round-trip limitations.

`EvaluationContext` holds per-run settings: clock, random seed, localization,
number-input locale, formatting, execution budgets and retention limits.
Capture time explicitly when reproducibility matters:

```rust
let context = poirot::EvaluationContext {
    as_of_time: std::time::UNIX_EPOCH,
    randomness_seed: 42,
    ..Default::default()
};
```

Reuse the context to keep these inputs fixed, or set `as_of_time` to
`SystemTime::now()` for a newly captured clock. Reexecuting `RAND` with the same
seed can reproduce the same value. New overlay runs refresh volatile formulas;
changed context inputs invalidate their consumers.

## Handle errors and resource limits

A formula error such as `#DIV/0!` is a successful `CellValue::Error` result.
`EvaluationFailure` means execution could not complete, for example because of
a budget limit or unsupported capability. Formula `IFERROR` cannot catch an
operational failure.

Calls within one evaluation share budgets and memoization. Each requested output
batch publishes new results only after successful completion. A failed batch
publishes no new values, preserves earlier successful calls, and makes that
instance unusable for further calculation.

For an overlay, create another evaluation to retry. For direct evaluation,
`begin(context)` starts a fresh run, resetting budgets and failure state while
retaining edits and compiled formulas. `clear_cache()` alone does not recover a
failed direct run. Inspect `usage()` and the failure's category before adjusting
`context.budgets` or `context.retention`; see [execution limits](../crates/poirot-eval/docs/evaluation-budgets.md).

## Apply edits and results

Both workspaces support `set_value`, `set_formula`, `set_style` and `remove_cell`.
`set_value` replaces an existing formula. `set_formula` clears its saved result.
`clear_cache()` drops calculated state; `reset()` also removes edits while retaining
compiled programs. In direct evaluation, use `begin(context)` when a fresh run is needed.

Export `changes()` or consuming `into_changes()` to publish authored edits and
successful result writes together. End the workbook borrow and call
`workbook.apply(changes)?`; the entire batch is validated and applied atomically.
Saved result writes preserve the formula, and spill writes preserve derived-cell
ownership. `WorkbookChange` values are owned and support Serde serialization.

`OverlayEvaluation::result_changes()` exports only completed result writes and
returns a `Result`; `into_result_changes()` also ends the evaluation borrow.
Use full workspace changes when the calculation included authored edits.
Saving the workbook is a separate step and never recalculates it.

## Ownership and preparation

Borrow a `Workbook` or supply an `Arc<Workbook>` to share a baseline between
independent workspaces. Each worker owns its mutable workspace. Thread transfer
depends on the handle and runtime services being `Send`; an Arc baseline also
requires a `Sync` workbook.

Compilation happens lazily. `compile_all()` prepares cell and defined-name
formulas ahead of use; `compiled_usage()` reports preparation work. It does not
calculate formula results. For retained results and invalidation, see
[incremental recalculation](recalculation.md).

Runtime function providers use `with_extensions` or `with_shared_extensions`.
See [extension hosts](../crates/poirot-eval/docs/extensions.md) for registration
and cacheability requirements.
