# Evaluation outcomes and budgets

`overlay.evaluation(&context)` creates one bounded execution lifetime. Set
`context.budgets` and `context.retention` before creating it. `cell`, `outputs`
and `all` return calculated values or a typed operational failure. A completed
value can contain a spreadsheet error such as `#NUM!` or `#REF!`; operational
failures bypass `IFERROR`/`IFNA`. Calls on one instance share the same budgets.
`finish()` exposes the final outcome and charged usage when a report is needed.

The subsystem's low-level `run_*` entry points also expose typed outcomes and
explicit budgets for engine integrations.

`EvaluationFailure::Budget(BudgetFailure)` retains the kind, limit, previously
consumed units, requested units, and active cell target. An unsupported helper
input domain can use `UnsupportedCapability { capability, source, detail }`.
This is distinct from an unknown formula function, which retains the existing
spreadsheet `#NAME?` behavior. The capability boundary does not register UDFs
or advertise additional formula implementations.

## Cost model

Defaults are 10,000,000 run work units, 1,000,000 direct cell work units, depth
128, 100,000 materialized values, 100,000 wildcard states per match, and
1,048,576 storage bytes per text buffer. A configured depth above 128 is rejected
as a configuration failure; zero budgets are valid and can abort execution.

Work units are deterministic operation counts and processing reservations,
not elapsed time, machine instructions, or a process-wide memory allocator.
The usage fields describe the charged categories:

- Expressions, cell reads, scalar operators, reference pairs, lookup candidate
  inspections, and actual calendar-loop steps each charge one unit per visit.
- Sparse range scans charge tree seeks and stored-cell visits in the requested
  region, including a final unsuccessful seek. Full-workbook discovery scans the
  stored workbook cells. Qualified sheet scans charge inspected sheets; named
  range lookup reserves two passes over the namespace.
- Value, area, dense matrix, result, and copied holiday collections check their
  logical element bound before growth, and charge reserved elements. These
  limits do not claim to equal allocator capacity or total live heap bytes.
- Text checks use the representation being retained or created: UTF-8 bytes, or
  two bytes per UTF-16 unit. A UTF-16-to-UTF-8 projection checks the projected byte
  length before allocation. Unicode case conversion and substitution likewise
  check output size before growing a buffer.
- Wildcard matching reserves token-by-position states before executing the
  dynamic program and checks each row buffer before allocation.

A formula frame charges its own direct work; evaluating a dependency opens a
separate frame. Cached dependency reads retain their own copy frame, so a prior
bulk visit does not change the caller's direct allowance. Whole-run work still
includes all those frames. Failed reservations do not increment usage, and
usage is not a claim that every reserved unit completed before a later abort.

Recursion bookkeeping is bounded by the hard depth ceiling. Formula parsing
and binding retain their own preparation limits; source text processing is
charged, but this execution meter does not instrument each parser allocation.
Reusable `FormulaPlan` inputs avoid parsing and binding during execution.
Native arithmetic, sorting, and standard-library string operations remain
bounded by their inputs; their machine-level instructions are not individually
counted.

`TEXT` compiles a legacy-compatible formatter plan and obtains its conservative
`render_byte_bound` before rendering. That reservation bounds each intermediate
or final UTF-8 render buffer, not total allocation traffic or compilation cost.
It can abort a small text budget even when the eventual display would be short.
No post-render length check is presented as a preallocation guarantee.

## Reading costs and choosing limits

| Question | Counter and lifetime |
| --- | --- |
| How much preparation did I request? | `overlay.compiled_usage()`: cumulative parse/bind attempts and prepared-source reuse for that overlay. Compare snapshots to measure one compilation. |
| Which formulas executed? | `evaluation.usage()`: per-run `RunUsage`, including formula executions, memo hits, and dynamic parsing of computed reference text. |
| What work was charged before completion or abort? | `EvaluationOutcome::usage()`: per-run `EvaluationUsage`, including reserved work. Failed charges do not increment these counters. |
| Which dependencies invalidate a result? | Consumed point/range/context traces retained by the overlay; source writes and volatile/context inputs invalidate reverse dependents. |

`parse_attempts` counts full parser calls made by preparation. Exact-source reuse
and lexical-template hits avoid those calls; `overlay.compiled_usage().template_hits` reports
the latter separately. Requesting `CompiledFormula::parsed()` can lazily reparse
source for introspection without incrementing preparation counters.
`RunUsage::dynamic_parse_attempts` counts runtime computed-reference parsing instead.
Zero formula executions therefore does not mean zero validation or parsing work.

Limit families are independent; raising a formula work allowance does not raise
cache or result-retention allowances:

| Limit family | Defaults | Configure through |
| --- | --- | --- |
| Formula execution | The `EvaluationBudgets` limits above | `EvaluationContext::budgets` |
| Per-instance memo and persistent result retention | 100,000 cells; 64 MiB logical bytes | `EvaluationContext::retention` |

Memo bytes charge 128 fixed units per accepted entry plus its owned text storage.
Cache bytes account for retained values, text, identities, and guard metadata;
validation units account for source/cached-text bytes, guard scans, and dependency
inspections. These are logical limits, not allocator-exact or process memory bounds.
Use the failure's category and requested/consumed amounts to identify which
allowance refused the work before changing a limit.

## Atomic output batches

`evaluation.outputs(&targets)` stages a whole requested output list before
publishing results and consumed dependencies to the overlay. Admission failure
publishes no new values from that batch. Earlier successful calls remain valid;
a failed instance cannot continue. The source workbook remains borrowed and
unchanged. Applying an exported change vector is a separate atomic workbook edit.

A new evaluation creates fresh stacks and memoization while reusing valid
persistent overlay results. Volatile and changed context consumers are dirtied
before reuse. Spill footprints are admitted together, including children and
ownership. Dependency metadata contributes to persistent logical retention units.

## Helper adapter boundary

Helpers must distinguish actual spreadsheet errors from operational failures.
Before allocating, an adapter must check collection counts, text byte bounds,
and any bounded algorithm states, then charge deterministic work through the
execution budget. A helper with no usable output bound must return an explicit
unsupported capability rather than allocating first and reporting usage later.
Reference arguments remain evaluator-owned views until requested materialization;
array inputs must have validated rectangular shape. A helper's result alone
must never be translated into a spreadsheet error merely because it is unsupported.
