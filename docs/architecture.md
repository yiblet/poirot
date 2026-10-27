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

## Evaluator layout

`src/eval/mod.rs` declares the folders below and re-exports the public evaluation
API. Each folder states its purpose and its "Depends on:" siblings in its `mod.rs`.
Evaluator code imports items from their defining module; a folder's `mod.rs`
re-exports serve only callers outside that folder. `super::` paths stay within one
top-level folder; imports from another folder use `crate::eval::<folder>::..`
(`tools/test_module_layout.py` checks both).

Folders are listed from the bottom layer up. A folder uses only folders listed
before it, except inside the recursion group.

| Layer | Folder | Purpose | Uses |
| --- | --- | --- | --- |
| 1 | `coordinates/` | Cell identity (`EvaluationTarget`) and run-local hash tables and memo keyed by sheet index and address | — |
| 1 | `inputs/` | Sparse scenario edits and the effective workbook reads that merge them with a baseline | — |
| 2 | `limits/` | Work budgets, usage records and run counters, memo retention limits, operational failures and outcomes, fallible allocation | coordinates |
| 2 | `materialization/` | Explicit cache publication policy and receipts | coordinates |
| 3 | `values/` | Value channels, reference areas, arrays, coercion, comparison, numeric results, budget-admitted copies, function failures, `CellEvaluation` | coordinates, limits |
| 4 | `config/` | Immutable inputs: policies, clock, random streams, calendar, locale, context fields, `EvaluationContext`, `EvaluationPolicy` | limits, values, materialization |
| 5 | `functions/` | Builtin identities and spellings, extension registrations, external function names, ATP names | config, coordinates, limits, values |
| 6 | `compile/` | Formula text to bound expression to bytecode, plus possible-result typing | functions, coordinates, limits |
| 6 | `observe/` | Execution events, bounded retention and consumed builtin calls | functions, coordinates, limits, values |
| 6 | `ports/` | Caller-implemented run ports: completed cells, consumed reads, cached results | functions, coordinates, limits |
| 7 | `reuse/` | Retained dependency evidence, the persistent result cache, spill roles and lookup prefixes | config, functions, coordinates, limits, ports |
| 8 | `session/` | `Session` and the executors for one cell lifecycle: cells, reads, projection, observation | layers 1–7, host, spill, vm |
| 8 | `vm/` | Bytecode execution, call admission, operators, references, selection and broadcasting | layers 1–7, session, host, adapters, spill |
| 8 | `spill/` | Dynamic-array discovery, settlement and publication | layers 1–7, session |
| 8 | `host/` | `CallHost` (adapter call boundary: arguments, admission, failures) and its executors; extension invocation | layers 1–7, session, vm, spill, adapters |
| 8 | `adapters/` | Per-family adapters over pure `poirot-functions` algorithms | layers 1–7, host, session, vm, spill |
| 9 | `run/` | `EvaluationRun`: one execution lifetime over a prepared workbook view | layers 1–8 |
| 10 | `entry/` | Public `run_*` entry points (including extension runs) and atomic recalculation | layers 1–9 |
| 10 | `overlay/` | `Overlay` and `Evaluation`: retained edits and incremental reuse across scenarios | layers 1–9 |
| 11 | `artifact/` | Versioned native snapshots that save and restore overlays | config, overlay |

Layer 8 is the evaluator's one recursion group: cells run formulas (`session` →
`vm`), formulas call adapters through the host (`vm` → `host` → `adapters`), and
adapters read cells and spills back through the session (`adapters` → `session`,
`spill`). These five folders depend on each other; every other dependency points
down. `tools/test_eval_dependencies.py` declares the group, checks each folder's
`Depends on:` line against its non-test imports and rejects any other cycle.

Inside a folder, each struct or enum lives in one file with all its impls. Behavior
that needs a whole `Session` or `CallHost` lives in an executor: a struct that
borrows it, is named for its job and is built at the call site, such as
`Vm::new(session)`. Examples are `Vm` (`vm/`), `CellEvaluator` (`session/cells/`),
`SpillDemand` (`spill/`) and `OperandReader` (`host/`).

`Session` (`src/eval/session/composition.rs`) holds only its component structs:
`RunSources` (workbook content, scenario inputs, compilation services),
`RunPolicies` (execution policies), `RunState` (execution storage and mode),
`ReuseHandles` (retained result caches and dependency recording) and `RunPorts`
(caller-installed observation ports). `RunState` reaches the `Runtime` storage, owned
by the run or borrowed from a persistent workspace (`Storage`), which holds one
component per concern: `Machine` (VM stacks, `vm/`), `BudgetMeter` and `RunUsage`,
`EvaluationStack` (cells and names under evaluation, the consuming target),
`ResultMemo` (memoized results and retention usage), `CallEffects` (random streams,
date coercion effects, extension trace), `SpillPreparation`, `LookupPrefixes` and
`ObservationState`. Each field has one path, such as `session.state.runtime.budget`.

## Follow a calculation

`book.evaluation().build()?.cell(target)` reaches the evaluator's direct scenario.
`book.overlay().build()?.evaluation(&context)?.cell(target)` uses the same execution
engine with dependency tracking and a cache retained across edits.

Both enter `CellEvaluator::cell` in `src/eval/session/cells/cell_evaluator.rs`:

1. Resolve the effective source, including scenario edits.
2. Record the read and reuse a valid cached result if available.
3. Otherwise evaluate the literal or compiled formula and its dependencies.
4. Restore the caller's state, record dependency evidence and notify observers.

In `src/eval/session/cells/`, `CellAttempt` carries execution state, `CellRead`
carries the result into completion and `CompletionContext` restricts completion to
bookkeeping and observer delivery. Spreadsheet errors are values. Operational
failures, such as exhausted budgets, are Rust errors. A failed output batch
publishes none of its new results.

`src/eval/compile/` turns formulas into bytecode; `src/eval/vm/bytecode_vm.rs`
defines the `Vm` executor that runs it. A compiled body holds instructions and a
label table. Deferred arguments preserve lazy function behavior; runtime operators
handle broadcasting.

## Types and spills

`src/eval/compile/typing/` infers possible scalar kinds, arrays, references, maximum
dimensions and effects alongside compilation. Unknown inputs stay conservative.
These facts help rule out impossible spills; they do not establish actual spill
ownership or replace runtime dependency tracking.

`src/eval/spill/` coordinates candidate discovery (`discovery.rs`), producer
execution (`producer_execution.rs`), overlap settlement (`settlement.rs`) and
admission (`admission.rs`). Readers demand settled ownership through the
`SpillDemand` executor (`spill_demand.rs`). Possible rectangles locate producers
that could reach a requested blank or range. Actual evaluated shapes determine
ownership. Authored nonblank cells block spills; competing producers use Poirot's
top-left priority. The [compatibility guide](compatibility.md) describes how that
policy differs from a promise of exact Excel compatibility.

## Reuse and edits

`Overlay` (`src/eval/overlay/incremental.rs`) retains edits, compiled formulas and
results between calculations. `src/eval/reuse/` records consumed cells and ranges
(`reads.rs`, `dependencies.rs`) and invalidates their readers after edits
(`cache.rs`). A dirty result can
remain cached when its dependencies recalculate to identical values. Context
changes and volatile functions follow their own invalidation rules. Export
includes only currently valid results.

Start with `tests/cell_lifecycle.rs` for public examples of dependency reads,
warm reuse, edits and failure recovery. Read [evaluation.md](evaluation.md) for
application workflows and [resources.md](resources.md) for budgets and profiling.
