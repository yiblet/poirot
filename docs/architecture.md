# Reading Poirot's calculation path

Start with one input and one dependent formula: `A1 = 3`, `B1 = A1*2`.
A request for `B1` resolves its effective source, reuses a validated result when
possible, otherwise evaluates `A1*2`, and completes with `6`. It does not change
the baseline workbook. Publication to a scenario cache and publication to a
workbook are separate steps.

## Component responsibilities

| Component | Owns | Does not own |
| --- | --- | --- |
| `src/lib.rs`, `poirot-workbook/src/scenario.rs` | Application vocabulary: `Workbook`, `Evaluation`, `Overlay`, builders and file export | Formula algorithms |
| `poirot-model` | Authored cells, worksheet coordinates, stored results and atomic workbook changes | Recursive evaluation |
| `poirot-parsing` | Formula syntax | Workbook reads and execution |
| `poirot-eval` | Preparation, dependency reads, budgets, execution, reuse and spill publication | File decoding and encoding |
| `poirot-coercionv2` | Spreadsheet argument coercion rules | Workbook and evaluator state |
| `poirot-functions-*` | Function algorithms over values and validated inputs | Workbook access and evaluator implementation |

Function adapters in the evaluator obtain references and dependencies, apply
coercion, and invoke the independent function crates. Keep workbook access out
of those algorithm crates; `tools/test_function_architecture.py` checks this
boundary. The lookup crate's `Array` and `Reference` constructors validate their
geometry, and their private fields preserve it after construction.

## Follow an ordinary cell request

The application call is `book.evaluation().build()?.cell(target)`. The workbook
wrapper forwards to `poirot_eval::overlay::Calculation`, then the direct
`overlay::Evaluation`. Its `perform` method attaches source and persistent
runtime storage to a `Session`. An incremental call starts with
`book.overlay().build()?.evaluation(&context)?.cell(target)` and reaches
`overlay::OverlayEvaluation::perform`, which attaches dependency tracking and a
cross-cycle cache. Both paths call the same `Session::cell` implementation.

Read `crates/poirot-eval/src/session_execution.rs` in this order:

1. **Resolve:** `Session::cell` calls `resolve_source`. The effective source
   overlays authored edits on the immutable workbook. Invalid coordinates or
   sheets become a spreadsheet `#REF!` value. Physical blanks remain distinct
   from formula cells, even when a formula's stored result is blank.
2. **Read and reuse:** `cell_from_source` creates a `CellAttempt` containing the
   target, caller and completion metadata. `CellAttempt::resolve_and_read`
   charges the cell read and records its dependency before `reuse` consults the
   caches. The reuse order is deliberate: a clean unowned blank certificate,
   spill ownership resolution, the current session memo, an installed external
   reader, then a clean incremental cache entry. A dirty retained entry requests
   execution with evidence validation rather than counting as a clean hit.
3. **Execute:** `CellAttempt::execute` checks recursion, enters the depth and
   budget frame, and sets the active dependency consumer. All fallible body work
   runs in `execute_source`; the enclosing method restores the caller, budget
   frame, depth and visiting state before returning. Evidence validation can
   avoid executing a formula when its dirty causes still produce the same
   values. Otherwise `execute_source` handles a blank, literal or prepared
   formula. It delegates compilation through `Session::prepare_source` and
   `SourcePreparation`, evaluates bytecode, normalizes the worksheet result,
   and admits it into local execution storage.
4. **Complete:** The value or operational failure travels with its attempt as a
   `CellRead`. `cell_from_source` restores the caller's expression demand before
   `CellRead::complete` records blank spill observations, marks eligible results,
   and delivers a completion notification through `CompletionContext`. That context
   borrows only preparation, budgets, visiting evidence, dependency bookkeeping
   and observer delivery; it cannot read workbooks, compile formulas or inspect
   caches. A failed attempt still notifies its
   observer. An observer failure cannot replace an existing operational failure.

A recursive `A1` read follows the same stages. Each formula uses worksheet demand
for its own result; it does not inherit a caller's temporary array collection
demand. Dependency attribution follows the active consumer and is restored after
the dependency returns. Cache provenance is retained even if copying a cached
text value exceeds the budget.

`range_cell` has a deliberate fast path for authored nonblank literals: it
charges the read and text, but retains no redundant memo value. Observers, spill
ownership, formulas and blanks route through the ordinary protocol. Supplied
prepared and bound expressions have their own entry points; they share expression
execution but do not perform ordinary source resolution and cache selection.

## Spill work and publication

An incoming blank or saved spill child may require producer discovery before an
ordinary cache lookup is valid. `CellAttempt::resolve_spills` temporarily assigns
ownership queries to that coordinate and restores the consuming cell before
returning, including on failure. See `spill_publication.rs` for the orchestration
and its restricted services:

- `Sources` and `Selection` resolve candidates and source readiness.
- `Engine` invokes producer execution and requests rollback through `Execution`.
- `Admission` reserves and admits values and ownership, recording rollback journals.
- `SourcePreparation` shares compilation with reached ordinary formulas.

Those boundaries prevent the discovery, execution and admission mechanisms from
all directly accessing the entire `Session`. The ordinary cell orchestrator
still coordinates session services; its stages make their ordering explicit.

Local admission is not a promise to an application that a whole request succeeded.
`overlay::OverlayEvaluation::perform` calls `publish` only after the requested
output or output batch succeeds. A failed batch retains none of its new results;
previously successful batches can remain. Its failure stays active for that
cycle. Starting a fresh cycle allows retry. The direct scenario follows the same
successful-request boundary when exporting its local results.

`Workbook::recalculate` goes further: it calculates and stages the result changes
before applying them atomically to the workbook. An operational abort writes none
of that recalculation's results. A spreadsheet error such as `1/0` is a successful
cell value (`CellError::Div0`), so it can be propagated, caught with `IFERROR`,
and published normally.

## Where to change behavior

For a new builtin, begin in its algorithm crate and evaluator adapter. For a
wrong reused value, inspect consumed-read evidence and invalidation before
changing expression execution. For an ownership problem, inspect spill
selection and admission. For a read lifecycle or observer bug, begin with the
stages above and keep failure and restoration order explicit.

`tests/cell_lifecycle.rs` exercises this path through the public API: dependency
reads, direct/incremental agreement, warm reuse, edits, spreadsheet errors,
atomic batch failure and recovery. Evaluator tests cover observer ordering,
consumed reads, memo retention, budgets and dynamic spill settlement in more detail.
