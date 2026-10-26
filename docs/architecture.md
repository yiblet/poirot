# Finding your way around Poirot

Start with `A1 = 3` and `B1 = A1*2`. Reading `B1` evaluates its dependencies and
returns `6`. Calculation leaves the source workbook unchanged until the caller
explicitly applies or exports results.

## Components

| Component | Responsibility |
| --- | --- |
| `poirot`: `src/lib.rs`, `src/workflow/` | Public workbook, evaluation, overlay and file workflows |
| `poirot`: `src/model/` | Sparse worksheets, stored results and atomic changes |
| `poirot`: `src/eval/` | Compilation, execution, dependency tracking and caching |
| `poirot-parsing` | Formula syntax independent of workbook state |
| `poirot-functions` | Pure function algorithms, scalar values, argument coercion and display formatting |
| `poirot-io` | File records, XLS/XLSX decoding and XLSX encoding |

Evaluator adapters resolve references and collect dependencies before calling pure
function modules. `tools/test_function_architecture.py` checks package dependencies
and keeps algorithm and codec modules independent of workbook implementations.

File codecs import directly into a `WorkbookSink` and export through borrowed
`WorkbookSource` views. `src/file_adapter.rs` implements these contracts over the
sparse model; crossing the boundary does not clone the workbook. IO owns file
records such as cell contents, styles and diagnostics. Functions own canonical
scalar values and calendar semantics, reused where they have identical meaning.

## From stored cells to calculated outputs

A worksheet is a sparse collection of source records, not a dense matrix of values.
For example, `A1 = 3` and `B1 = A1*2` produce two stored `Cell` records. The first
holds a literal input; the second holds formula text and a saved result that may
still be blank or stale. An unstored `C1` has no record at all. A stored blank can
still carry formatting, so absence and a blank value are separate facts.

| Type | Role |
| --- | --- |
| `CellAddress` | Worksheet-local row and column. `EvaluationTarget` adds a resolved sheet index for calculation. |
| `Cell` | Source record: literal or formula, saved value, style and optional shared-formula provenance. |
| `CellValue` | Typed scalar used for both inputs and results: blank, number, text, boolean or spreadsheet error. |
| `CellError` | A formula-visible error such as `#DIV/0!`; operational failures use Rust errors instead. |
| `CellStyle` | Presentation retained independently of the value; formatting a number does not turn it into text. |
| `CellEvaluation` | A calculated output paired with its target and optional source formatting, without changing the source record. |

`Worksheet` stores records in `CellStore<Cell>`. Its `WorksheetCells` view and
`CellIter` traverse stored coordinates in row-major order, skipping absence but
including explicit blanks. `CellRange` selects an interval in that address order,
not a rectangle. `ColumnCells` and `OccupiedRows` narrow traversal without expanding
missing coordinates. `CellRead` gives worksheets and generic stores the same
borrowed sparse-read interface; it supplies no calculation.

For rectangular model queries, `query::WorkbookView` applies a `QueryPlan` of
`RangeMembership` areas. Membership includes absent coordinates; `StoredCells`
and `StoredCellsReverse` visit only records that exist. These reads expose saved
results, not freshly calculated values. Formula cells with blank saved results
remain formulas when `BlankCounts` distinguishes them from explicit blank inputs.

Formula syntax keeps a separate identity: `CellReference` retains an optional sheet
name and `$` anchors for copy translation; `CellCoordinates` is the parser's decoded
token form. Evaluation resolves references against a workbook before reading cells.
Neither syntax type owns a stored cell or a calculated result.

## Follow a calculation

`book.evaluation().build()?.cell(target)` reaches the evaluator's direct scenario.
`book.overlay().build()?.evaluation(&context)?.cell(target)` uses the same execution
engine with dependency tracking and a cache retained across edits.

Both enter `Session::cell` in `src/eval/session_execution.rs`:

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
