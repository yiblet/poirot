# Incremental recalculation

[Documentation](README.md)

An `Overlay` retains calculated results between evaluations. It records which
inputs each formula actually consumed and invalidates affected results when
those inputs change. Use it for repeated scenarios over a stable workbook.
See [evaluation and overlays](evaluation.md) for a runnable scenario loop.

## What an edit invalidates

An input change invalidates that cell and its transitive consumers. Dependencies
include point reads and range membership, including absent coordinates. Inserting
a value into a previously empty member of a consumed range therefore invalidates
the aggregate. Unrelated results remain reusable.

Only evaluated branches establish value dependencies. If a formula switches
branches, successful recomputation replaces its old dependencies. An input used
only by the old branch then stops invalidating that result. Ordinary input edits
retain compiled formulas even when calculated results are invalidated.

Dynamic arrays retain complete spill footprints. Invalidating a producer removes
its children and downstream results. Blockers, new producers and shrinking
footprints participate in invalidation so stale spill children are not retained.

## What a new evaluation refreshes

Starting `overlay.evaluation(&context)` refreshes volatile and opaque formulas
and their dependents. Changes to clock, seed, localization, numeric-input locale
or formatting invalidate formulas that consumed those settings. Reusing a seed
can reproduce random values even though RAND executes again.

Extension providers declare deterministic, versioned, volatile or opaque
cacheability. Their declaration and tracked inputs determine reuse. Follow the
[extension contract](../crates/poirot-eval/docs/extensions.md) when supplying providers.

Imported formula saved values and manual cache writes are not trusted calculation
results. Reuse requires evaluator-produced values with their dependency information.

## Measure and control reuse

`evaluation.usage()` separates formula executions, run-local memo hits and
persistent cache hits. Lookup prefix hit/scan counters expose reuse for supported
numeric/boolean exact MATCH, HLOOKUP and VLOOKUP searches, even when the lookup
formula itself executes again. Text and approximate searches use the ordinary
search path.

`compiled_usage()` reports preparation independently of execution. `compile_all()`
can move preparation ahead of a run but does not populate calculated results.

`clear_cache()` discards evaluated state. `reset()` also removes source edits.
Both keep reusable compiled programs. Execution and retention limits are separate;
see [budgets](../crates/poirot-eval/docs/evaluation-budgets.md) when a large scenario
is refused. Successful batches publish their results together; a failed batch
does not partially add results to the overlay.

Direct `Evaluation` clears local calculated results after each effective source
edit and retains no dependency graph. It is suitable when persistent result reuse
would add little value. Both modes share the same calculation engine.

For engine integration details, see the component's
[dependency semantics](../crates/poirot-eval/docs/incremental-cache.md).
