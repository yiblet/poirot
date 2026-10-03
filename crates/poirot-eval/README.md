# poirot-eval

Evaluate through an editable direct `Evaluation`, or a persistent `Overlay`
and borrowed `OverlayEvaluation`. Both share source lookup, compilation, output
projection, and the same calculation/spill engine. Direct evaluation clears local
results on source edits and stores no dependency graph; overlay writes selectively
invalidate retained results. `EvaluationContext` supplies time, seed, localization,
formatting inputs and execution limits. The source workspace owns policies.
The root `poirot_eval::EvaluationContext` and `overlay::EvaluationContext` are the
same full context type. Lower-level `EvaluationRun` and `RunConfiguration` APIs
use `EvaluationInputs` for clock, seed and localization; `context.inputs()`
explicitly projects those inputs without transferring formatting or limits.

```rust
use poirot_eval::overlay::{Overlay, EvaluationContext};
use poirot_model::{CellValue, Workbook};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut wb = Workbook::new();
    wb.add_sheet("S")?.set_formula("B1".parse()?, "A1+1", CellValue::Blank)?;
    let mut overlay = Overlay::with_workbook(&wb).build()?;
    let input = overlay.target("S!A1")?;
    let output = overlay.target("S!B1")?;
    overlay.set_value(input, 4.0)?;
    let context = EvaluationContext::default();
    let mut evaluation = overlay.evaluation(&context)?;
    assert_eq!(evaluation.cell(output)?, CellValue::Number(5.0));
    drop(evaluation);
    let changes = overlay.into_changes();
    assert!(!changes.is_empty());
    Ok(())
}
```

`Overlay::changes()` and `Evaluation::changes()` export authored edits together
with successful result writes; `into_changes()` consumes the workspace.
`OverlayEvaluation::result_changes()` exports only successful result writes from
that borrowed evaluation; `into_result_changes()` also ends its borrow.

Use `outputs(&targets)` for a list or `all()` for the sparse effective workbook.
Compilation is lazy; `compile_all()` is optional. Interleave source edits and new
evaluations on the same overlay. Writes evict affected results through reverse
point and range dependencies. Each new instance refreshes volatile/opaque cells
and actual consumers of changed context inputs. Selected branches replace their
old dependency traces after successful recomputation.

For lower-level program reuse, `FormulaCompiler` parses, binds, and interns
formula bytecode under a preparation policy. `compile_workbook()` returns a
`CompiledWorkbook`: an immutable index of cell/name programs checked against
source topology, not an evaluation schedule. The overlay owns a compiler and
keeps evaluated values in its separate dependency-aware cache. `CompilerUsage`
counts parse/bind attempts and program reuse, not formula execution.

Bytecode is the only production execution representation, including caller-owned
`bound::FormulaPlan` values, adapter inputs, and references parsed by INDIRECT.
`CompiledFormula::bytecode()` always returns a program. `parsed()` returns an owned
inspection view; `FormulaPlan::expression()` likewise constructs an owned bound
view without retaining a second execution tree. Preparation failures remain
explicit and deferred until reached, so unused branches and unused invalid names
remain unevaluated. The tree interpreter is compiled only for unit-test parity
checks of values, dependency reads, observations, and budget accounting.

Prepared snapshot compatibility is owned by
`overlay::snapshot::COMPATIBILITY_REVISION` (currently 5). Applications must reject
incompatible prepared payloads before decoding them; this is a same-engine cache
contract, not a promise of historical bytecode migration.

Direct evaluation starts with `Evaluation::with_workbook(handle).build(context)`.
It supports the same setters, `cell`, `outputs`, `all`, `compile_all`, and change
export. `begin(context)` starts a new run while retaining source edits and bytecode;
ordinary edits clear results but preserve cumulative work budgets and failure state.
Use `&Workbook` or `Arc<Workbook>` for either workspace. Both can be transferred
between threads when their handles and attached services are `Send`.

Snapshots expose Serde on borrowed views rather than runtime owners. They retain
bytecode, exact references, source edits, results and dependency/spill metadata.
Restoring shared programs does not parse formula text. `Overlay::from_snapshot`
returns `(Workbook, WorkspaceState)`; attach the state to an exactly matching
baseline handle. `attach_with_extensions` reattaches the matching runtime registry.
Direct snapshots omit local results; `into_evaluation(handle, context)` reuses
source/bytecode and starts with fresh results. Neither attachment creates a
self-referential owner.

[Dependency semantics](docs/incremental-cache.md),
[evaluation identity and reuse](docs/evaluation-semantics.md),
[execution budgets](docs/evaluation-budgets.md),
[dynamic arrays](docs/dynamic-array-results.md), and
[extension hosts](docs/extensions.md) describe the underlying contracts.
Typed spreadsheet errors remain values; operational failures bypass `IFERROR`.
Unknown functions return `#NAME?`. `DynamicSpill` supports scalar and singleton-axis
operator broadcasting. Arbitrary external links and general function broadcasting
remain unsupported.
