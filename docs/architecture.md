# Finding your way around Poirot

Start with `A1 = 3` and `B1 = A1*2`. Reading `B1` evaluates its dependencies and
returns `6`. Calculation leaves the source workbook unchanged until the caller
explicitly applies or exports results.

## Components

| Component | Responsibility |
| --- | --- |
| `src/lib.rs`, `poirot-workbook` | Public workbook, evaluation, overlay and file workflows |
| `poirot-model` | Cells, worksheets, stored results and atomic workbook changes |
| `poirot-parsing` | Formula syntax |
| `poirot-io` | File decoding and encoding |
| `poirot-eval` | Compilation, execution, dependency tracking and caching |
| `poirot-coercionv2` | Spreadsheet argument coercion |
| `poirot-functions-*` | Function algorithms independent of workbook and evaluator state |

Evaluator adapters resolve references, collect dependencies and coerce arguments
before invoking the function crates. `tools/test_function_architecture.py` checks
that those crates stay independent of the evaluator.

## Follow a calculation

`book.evaluation().build()?.cell(target)` reaches the evaluator's direct scenario.
`book.overlay().build()?.evaluation(&context)?.cell(target)` uses the same execution
engine with dependency tracking and a cache retained across edits.

Both enter `Session::cell` in `crates/poirot-eval/src/session_execution.rs`:

1. Resolve the effective source, including scenario edits.
2. Record the read and reuse a valid cached result if available.
3. Otherwise evaluate the literal or compiled formula and its dependencies.
4. Restore the caller's state, record dependency evidence and notify observers.

`CellAttempt` carries execution state; `CellRead` carries the result into completion.
`CompletionContext` restricts completion to bookkeeping and observer delivery.
Spreadsheet errors are values. Operational failures, such as exhausted budgets,
are Rust errors. A failed output batch publishes none of its new results.

Compilation lives in `compiler.rs` and `bytecode.rs`; execution lives in
`bytecode_vm.rs`. A compiled body holds instructions and a label table. Deferred
arguments preserve lazy function behavior; runtime operators handle broadcasting.

## Types and spills

`typing.rs` infers possible scalar kinds, arrays, references, maximum dimensions
and effects alongside compilation. Unknown inputs stay conservative. These facts
help rule out impossible spills; they do not establish actual spill ownership or
replace runtime dependency tracking.

`spill_publication.rs` coordinates four smaller services: candidate discovery,
producer execution, overlap settlement and admission. Possible rectangles locate
producers that could reach a requested blank or range. Actual evaluated shapes
determine ownership. Authored nonblank cells block spills; competing producers
use Poirot's top-left priority. The [compatibility guide](compatibility.md) describes
how that policy differs from a promise of exact Excel compatibility.

## Reuse and edits

`overlay/dependencies.rs` records consumed cells and ranges and invalidates their
readers after edits. A dirty result can remain cached when its dependencies
recalculate to identical values. Context changes and volatile functions follow
their own invalidation rules. Export includes only currently valid results.

Start with `tests/cell_lifecycle.rs` for public examples of dependency reads,
warm reuse, edits and failure recovery. Read [evaluation.md](evaluation.md) for
application workflows and [resources.md](resources.md) for budgets and profiling.
