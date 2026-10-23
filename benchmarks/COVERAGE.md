# Representative speed coverage

`coverage.py` generates 57 deterministic cases: the unchanged seven original
workloads, all eleven existing range neighbors, and 39 additional cases.
`expanded.py` runs them in the same per-engine Docker isolation and retains
capability failures and incorrect results alongside successful measurements.
This is a speed suite with correctness checks, not a complete compatibility test.

```sh
python3 benchmarks/expanded.py --sizes 16 64 256 \
  --engines poirot-evaluation poirot-overlay ironcalc \
  --samples 3 --warmup 1 --cpuset-cpus 0 --output /tmp/poirot-coverage
python3 benchmarks/expanded.py --sizes 16 \
  --engines hyperformula-array apache-poi libreoffice \
  --samples 1 --warmup 0 --cpuset-cpus 0 --output /tmp/external-coverage
```

Choose an available CPU. `--cases NAME ...` selects an explicit subset; the full
suite remains the default. `--no-build` reuses recorded image IDs and may therefore
use older adapter source. Schema2 reports and exact required phases reject old
schema1 adapters. To measure the current checkout, build its images first.

For a compiler-matched Rust comparison, build both Rust executables with one
compiler, freeze `poirot-speed`, `ironcalc-speed` and `rustc-version.txt` in a small
directory, then pass `--rust-bin-dir DIRECTORY`. Both run through read-only mounts
in `--rust-runtime-image` (default `poirot-bench-ironcalc:local`). Verify that the
runtime can execute those binaries; keep compiler/build logs and binary hashes.
The runner records hashes, compiler stamp, image configuration, source hashes,
commands and resource limits. This avoids silently comparing a current host
Poirot build against a Rust executable built with an older Docker compiler.

## Costs covered

| Family | Added cases | What varies |
| --- | ---: | --- |
| Graphs | 3 | Diamond joins with one/many outputs; an uncapped dependency chain. |
| Spatial discovery | 2 | Sparse anti-diagonal demands; tall-narrow then short-wide demands. |
| Reductions | 2 | Sliding windows and overlapping rectangles with moving starts. |
| Criteria | 4 | Repeated/changing `SUMIF`, multiple `SUMIFS` conditions, wildcard text criteria. |
| Lookups | 9 | Early/late/missing/duplicate/approximate queries, unique axes, wildcard formula-text keys, stored literal-text keys, live return columns. |
| Coercion | 1 | Actual division errors, absent cells and numbers; edits replace an error formula and create a previously absent source. |
| Text | 2 | Distinct concatenations, replacement and position search, observed through lengths/positions. |
| Dates | 2 | Date differences and business-day calendars across month/year boundaries. |
| Statistics | 3 | Median and population variance under both modern `VAR.P` and legacy `VARP` names. |
| Finance | 1 | Repeated discounted cash-flow ranges with `NPV`. |
| Arrays | 2 | Pairwise range products; small matrix multiplication and reduction. |
| Spills | 6 | Fixed/input-sized extents, many producers with one/all outputs, unknown width, overlapping possible bounds with disjoint actual footprints. |
| Source changes | 2 | Cross-sheet dependencies and formula-source replacement. |

Each size changes meaningful graph/range/output dimensions; it is not a Cartesian
product of every function and layout. The original seven and all eleven neighbors
keep their formula spellings, output order and edit ordinals. Existing neighbors
include horizontal/rectangular/descending prefixes, identical ranges, scalar
formula members, mixed text/Boolean/blank members, alternating reductions and
fixed ranges at different physical positions.

Growth/shrink tests read a numeric sum beyond the original spill extent, so stale
children or ignored size edits cannot pass by returning unchanged old entries.
Possible-rectangle cases have genuinely intersecting conservative bounds and
disjoint actual extents. They do not impose Poirot's top-left conflict policy on
other engines. Fixed dimensions and input-sized dimensions have separate probes.

## Correctness and capability boundaries

Inputs and expected values are original. Arithmetic, index selection, Gregorian
weekday counting, population variance and discounting are computed independently
in Python; expected results are never taken from Poirot. The relevant semantics
are described by Microsoft's [VLOOKUP](https://support.microsoft.com/en-us/excel/functions/vlookup-function),
[NETWORKDAYS](https://support.microsoft.com/en-us/excel/functions/networkdays-function),
[VAR.P](https://support.microsoft.com/en-us/excel/functions/var-p-function),
[NPV](https://support.microsoft.com/en-us/excel/functions/npv-function) and
[SEQUENCE](https://support.microsoft.com/en-us/excel/functions/sequence-function)
documentation. No upstream implementation, examples or fixtures were copied.

Schema2 keeps finite numeric outputs and the existing tolerance. Text workflows
have useful numeric observables. Stored literal text uses `{"text":"..."}`;
a plain string remains a formula beginning with `=`. Formula-result text and
literal text are separate lookup cases because the former also exercises formula
evaluation and dependencies. The missing-key `IFERROR` workflow is accompanied
by positive unwrapped `VLOOKUP` cases; unsupported lookup implementations cannot
be admitted merely because their errors produce a matching sentinel.

Feature probes exercise the operation itself. Only an engine/feature failure
explicitly listed in `CAPABILITY_GAPS`, matching its reviewed diagnostic, can be
reported as unsupported. Other errors, numeric mismatches, missing phases,
timeouts and stale protocols remain failures. A failed group is bisected to
locate the failing case; its partial timings are not accepted. Every invocation,
including failed groups and timeouts, keeps its workload, command and diagnostics.
The final report retains every engine/case outcome. A mismatch makes the command
exit nonzero and contributes no successful timing for that case.

`hyperformula-array` explicitly enables `useArrayArithmetic`; its identity is
`automatic-array-arithmetic`. The existing `hyperformula` selection and schema1
runs retain the default scalar arithmetic setting. A modern-array comparison
must state which workflow was used. POI and Calc use their documented existing
formula-evaluator/token APIs; a missing spill child is a limitation of that
workflow, not proof that every possible API in that application lacks arrays.

The narrow capability manifest is based on pinned-adapter probes:

| Workflow | Observed capability gap |
| --- | --- |
| POI 5.5.1 FormulaEvaluator | Explicit unimplemented `SEQUENCE`; parser rejects `VAR.P`, while `VARP` remains a separate control. |
| Calc 26.8.1.1 setTokens | `SEQUENCE` anchor returns 1, but its requested child remains blank. This does not characterize other Calc APIs. |
| HyperFormula 3.5.0 array arithmetic | Fixed dimensions work; input-sized `SEQUENCE` returns `#VALUE!` with “Cell range not allowed.” |

Unexpected wildcard-selection or referenced-Boolean differences remain numeric
mismatches, not capability exclusions. Raw diagnostics determine the report;
this table is not a blanket permission to skip a failing function family.

## Timing and interpretation

Schema2 adds `unchanged_read` after the initial calculation, before both edits.
IronCalc skips `evaluate()` and Calc skips `calculate()` during this phase.
Poirot and POI repeat their ordinary cached output requests. Construction,
unchanged reads and each edit remain separate rows. Source-formula setters and
native invalidation are inside the edit timer. JSON, validation and disposal
remain outside. LibreOffice includes document population and Java/UNO transport;
its numbers describe that workflow rather than isolated arithmetic.

`poirot-workbook-cell` calls the public `eval::run_cell_with_configuration` for
each output against the same authored workbook. It creates fresh execution state
per call and supports cross-sheet references. Its repeated phase is named
`warm_evaluate_read`, not `unchanged_read`, because formulas run again. There is
no implied Worksheet evaluation method. Per-cell Evaluation/Overlay remain the
original gate; batch routes and this stateless route are separate controls.

The runner writes raw reports, per-case/phase/size medians, p95/min/max,
descriptive IronCalc ratios and adjacent-size growth. It does not average away
outliers. Its few within-process rounds are exploratory screen samples, not
independent trials or evidence of parity. Bisection is correctness diagnosis,
not a way to add independent statistical repetitions.

For a targeted timing decision, use a small pilot of independent paired process
runs, choose a fixed confirmation count from its variance, then test mean paired
log timing ratios with a two-sided t-test and 97% interval (`p < .03`). Use at most
ten process pairs including pilots; stop inconclusive at that cap. Inner samples
and warmups are correlated and do not increase the independent trial count.
An inconclusive result is not proof of equality. Correct multiple comparisons if
making a suite-wide claim; retain absolute effects as well as ratios.

Captured Poirot clock/seed settings stay fixed. Actual context-change timing,
randomized/large-string lookup data, broad source-edit batches and large dense
matrix sizes remain explicit follow-up coverage. External engines do not expose
equivalent deterministic context controls through these adapters; ordinary input
edits are not presented as a substitute. File I/O and private data are excluded.
