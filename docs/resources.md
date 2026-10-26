# Resource configuration

Loading and ordinary calculation need no application quotas. Set limits when your application needs them:

```no_run
use poirot::Workbook;
use poirot::calculation::EvaluationLimits;
use poirot::files::ReadOptions;

let options = ReadOptions::default().max_input_bytes(256 * 1024 * 1024);
let book = Workbook::load().options(options).open("model.xlsx")?;
let mut calculation = book.evaluation()
    .limits(EvaluationLimits::default().max_work(10_000_000))
    .build()?;
let output = calculation.target("Summary!B12")?;
let value = calculation.cell(output)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Named setters replace one setting. A complete `options` or `limits` object replaces that configuration; later named setters override individual settings. Options are reusable configuration, never consumed counters. Zero means zero allowance. `without_*_limit()` removes an override, and optional inspectors return `None` for no quota.

Loading defaults now impose no input-byte, expanded ZIP, entry-count, XLS claim, or snapshot size quota. Previously, defaults included 32 MiB input/entry limits, 128 MiB expanded package limits, and separate XLS and snapshot caps. XLSX export defaults remain unchanged. Snapshot save builders retain their historical 64 MiB output default; `SnapshotLimits::default()` describes unlimited loading, while `SnapshotLimits::for_writing()` explicitly selects historical snapshot output quotas.

| Control | What it counts | Scope and reset | Check and refusal |
| --- | --- | --- | --- |
| `ReadOptions::max_input_bytes` | Source bytes, including compression | One load | Before decode, including a second check after file read; `IoError::Quota` |
| `max_zip_entry_bytes` | Expanded bytes in each ZIP entry | Each entry | Catalog claims and actual recovery expansion; `IoError::Quota` |
| `max_zip_total_bytes` | Sum of declared expanded entry bytes | One package | Catalog, before expanding parts; `IoError::Quota` |
| `max_zip_entries` | ZIP catalog entries | One package | Before recovery expansion; `IoError::Quota` |
| XLS `ResourceBudget` | Named stream, record, metadata, string and decoded cell-text claims | One load; string/record options retain their documented per-object or aggregate meanings | Preflight before backend decode; `PreflightError::Resource` |
| Snapshot file/decoded quotas | File bytes, cumulative typed values, or cumulative copied text/bytes | One load, shared across source and prepared sections | Before typed collection admission or owned copy; `SnapshotError::Quota` |
| `EvaluationLimits::max_work` | Engine operations actually charged, including admitted kernel estimates | One explicit cycle | Before the operation; `EvaluationFailure::Budget` |
| `max_formula_work` | Direct work in a formula, excluding dependency formulas | Each formula frame | Before the operation; `EvaluationFailure::Budget` |
| `max_collection_values` | Logical elements in one admitted collection, including intermediate arrays and internal collections | Each collection, including cached result admission | Before allocation/admission; `EvaluationFailure::Budget` |
| `max_text_bytes` | Bytes in one variable text buffer; UTF-8 bytes or two bytes per UTF-16 unit | Each buffer, including cached text | Before allocation/copy; `EvaluationFailure::Budget` |
| `max_wildcard_states` | Visited states in one wildcard match | Each match | Before visiting another state; `EvaluationFailure::Budget` |
| Memo `max_cached_cells` / `max_logical_memo_bytes` | Retained cell results and logical storage, including the mode's dependency metadata | Lifetime of the retained memo | Before publication; tightening below current usage refuses before starting a cycle |

These counts are not a process memory limit. Multiple collections coexist. Memo limits exclude compiled programs and derived lookup indexes. Retained overlay byte accounting includes dependency evidence; direct memo accounting uses 128 fixed logical units per entry plus owned text. Neither is allocator-exact RSS.

Several output reads share one cycle. Direct `Evaluation::start_cycle(context)` resets work and a previous refusal, preserves edits and compilation, and clears calculated values. Clearing the cache or editing a cell does not replenish work. An overlay's `evaluation(&context)` starts an explicit cycle; valid retained results survive it. Changing only quotas does not invalidate semantic caches. A refused cycle stays refused until a new cycle begins. Retention follows the state, independently of the cycle; Poirot refuses a tighter retention quota instead of promising automatic eviction.

`Workbook::evaluation().build()`, `Overlay::evaluation`, and workbook recalculation omit detailed profiling. Finite work quotas still enable the accounting required for enforcement. Select `.profiling(true)` on the direct builder or `Overlay::evaluation_with_profiling(&context, true)` to collect category and execution counters. `EvaluationReport::usage` and `execution` are `None` when profiling was not requested. The component `poirot::eval::overlay` APIs retain their existing detailed reporting contracts.

`EvaluationContext::limits` is the sole calculation quota configuration. The
lower-level engine functions still accept `poirot::eval::EvaluationBudgets` where
an explicit integer representation is needed; convert from `EvaluationLimits`
with `Into`. Context fields use the optional limits directly, including explicit
maximum-valued quotas. Use `..Default::default()` when constructing a context
with selected overrides.

The pure modern-text algorithm error `Limit` is now split into `SizeOverflow`, `AllocationFailure`, and `InvalidGeometry`; applications that match this component error must update those matches. Matrix algorithm errors likewise distinguish overflow and allocation from their existing explicit budget refusal.

Representation overflow, a fallible allocation refusal, and native execution guards are operational failures, distinct from configured quotas and unsupported features. `EvaluationFailure::Resource` identifies those causes without inventing consumption counts. Formula handlers such as `IFERROR` cannot catch operational failures. Numerical algorithms retain their existing convergence guards and worksheet-error semantics.

Formula preparation retains its existing 1,024-expression admission ceiling as well as parser nesting limits. The Pratt parser can build a long left-associated AST without increasing parser call depth; later recursive binding, cloning, and destruction still depend on the expression ceiling to bound that spine. Removing it requires a separately enforced safe AST-height limit. This is an engine preparation constraint, separate from runtime work quotas. Prepared-snapshot validation no longer imposes its former independent million-item and 64 MiB buffer quotas; it still validates actual instruction indices, lengths, source identity, and cache evidence.

Evaluation retains a 128-frame safe nesting ceiling. Native snapshot decoding retains a 256-level recursive decoder ceiling; a caller may lower it but cannot raise it. Excel dimensions, text rules, and representable sizes still apply without quotas. Native regex execution has separately named `RegexSettings`: match-loop steps, backtracking frames, and heap kibibytes. They interrupt PCRE2 independently of estimated evaluator work. The default guards remain 1,000,000 steps, 256 frames and 8 MiB; callers can change them through `EvaluationContext::regex_settings`. PCRE2 consumption is unavailable, and its native ceilings still apply. JIT stays disabled so these guards apply.
