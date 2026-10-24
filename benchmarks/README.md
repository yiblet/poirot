# Calculation speed benchmarks

Compare Poirot with other spreadsheet engines on identical, generated models.
Each engine has its own Dockerfile and runtime. This suite measures calculation
workflows; it does not read or write Excel files.

From the repository root, with Python 3.11+ and Docker running:

```sh
python3 benchmarks/run.py
```

This builds five images, runs all eight engine/mode combinations serially,
and writes `benchmarks/results/<timestamp>/summary.md`. Rust uses release builds.
The first build downloads dependencies; subsequent builds use Docker's cache.

For a quick smoke run or a larger comparison:

```sh
python3 benchmarks/run.py --size 16 --samples 3 --warmup 1
python3 benchmarks/run.py --size 2000 --samples 5 --warmup 3 --cpuset-cpus 2
```

Choose a CPU your host permits. Every engine receives the same CPU quota
(`--cpus 1`) and memory/swap limit (`--memory 2g`). Benchmarks run without network
access. Avoid other CPU-intensive work during timing. Engine order is shuffled
with a recorded seed; repeat with different `--seed` values to investigate drift.

## Engines

| Selection | Dockerfile | Pinned engine | Calculation behavior |
| --- | --- | --- | --- |
| `poirot-evaluation` | [poirot](poirot/Dockerfile) | Local checkout | Demand-driven outputs; edits clear calculated results. |
| `poirot-overlay` | [poirot](poirot/Dockerfile) | Local checkout | Retains dependency information and unaffected results across edits. |
| `poirot-evaluation-batch` | [poirot](poirot/Dockerfile) | Local checkout | Evaluation with one ordered `.outputs()` request. |
| `poirot-overlay-batch` | [poirot](poirot/Dockerfile) | Local checkout | Overlay with one atomic `.outputs()` publication. |
| `ironcalc` | [ironcalc](ironcalc/Dockerfile) | IronCalc 0.9.0 | Raw `Model` with explicit whole-model `evaluate()`. |
| `hyperformula` | [hyperformula](hyperformula/Dockerfile) | HyperFormula 3.5.0 | Automatic calculation during construction and input edits. |
| `apache-poi` | [apache-poi](apache-poi/Dockerfile) | Apache POI 5.5.1 | In-memory `XSSFWorkbook`; demand-driven `FormulaEvaluator`, with `notifyUpdateCell` after edits. |
| `libreoffice` | [libreoffice](libreoffice/Dockerfile) | LibreOffice 26.8.1 | In-memory Calc document over Java UNO; manual population, dirty `calculate()` after each edit. |

Select comparisons with `--engines poirot-overlay ironcalc hyperformula`.
Use `--no-build` to reuse images you already built; this can benchmark older source
than the current checkout. Image IDs are recorded, and execution uses those IDs.
All Rust comparison packages live in a separate workspace and lockfile; they add
no comparison-engine dependencies to Poirot. See [licenses and provenance](LICENSES.md).

To build or invoke an image individually:

```sh
docker build -t poirot-bench-poirot:local -f benchmarks/poirot/Dockerfile .
docker build -t poirot-bench-ironcalc:local -f benchmarks/ironcalc/Dockerfile .
docker build -t poirot-bench-hyperformula:local -f benchmarks/hyperformula/Dockerfile .
docker build -t poirot-bench-apache-poi:local -f benchmarks/apache-poi/Dockerfile .
docker build -t poirot-bench-libreoffice:local -f benchmarks/libreoffice/Dockerfile .
docker run --rm -i --network none poirot-bench-poirot:local overlay < benchmarks/results/RUN/workloads.json
```

## What is timed

Every workload has fresh-model warmups, then measured rounds in the same process.
These rounds are correlated; independent statistical trials require separate
process runs. Each
round constructs a new model, calculates the requested outputs, makes two input
edits in order, and reads outputs after each edit.

- **`build_calculate_read`:** construction, cell population, parsing/compilation,
  native initial calculation, and requested result collection. Engine-side
  conversion from the shared cell list is included. No model is reused here.
- **`edit_calculate_read`:** one changed input, invalidation/native recalculation,
  and requested result collection on the model from the initial phase. Each edit
  is reported separately. Edits change values, so this is not repeated cache reads.

Process/container startup, module loading, JSON input decoding, output validation,
serialization, and model disposal are outside the timers. Node JIT and garbage
collection that occur during measured operations remain included. Warmups use the
same process and discard their measurements; increase warmups and samples when
investigating JIT variability.

The Java adapters exclude JVM startup and JSON decoding. LibreOffice also starts
a private headless office process and connects a local UNO pipe before timing.
Each round then creates a new in-memory document; no spreadsheet file is loaded
or saved. Its document creation, formula parsing, native calculation, and every
Java/UNO bridge call needed to populate and collect outputs are included. This
transport overhead can dominate small models and many-output workloads; these
results describe the UNO workflow, rather than isolated Calc arithmetic speed.
The office process survives the suite, while each document is disposed outside
timing. The temporary profile belongs to this container alone and is removed on
exit. The pinned official Linux archive currently targets x86-64.

Calc parses the original English Excel A1 formulas using its public
`FormulaParser` with the public `XL_ENGLISH` opcode map (including comma
separators) and `XL_A1` references, then sets native tokens. Both
steps are inside construction timing. Automatic calculation stays disabled;
`calculate()` evaluates dirty cells initially and after each edit. POI evaluates
requested outputs using native caching and notifies the evaluator of the changed
source; it does not clear unrelated cached results. Both reject text, blanks,
errors, and nonfinite outputs before publishing any report.

These are native API workflow comparisons. Eager and demand-driven engines do
different internal work; the suite does not pretend parsing or graph construction
can be separated identically in each. Every formula in each original workload is reachable
from its requested outputs. The [expanded suite](COVERAGE.md) adds unchanged
reads, source changes, spatial demands and additional function families; some
spatial controls deliberately request only part of a larger model. File I/O,
memory benchmarks and parallel throughput remain separate workflows.

## Broader coverage

The original seven-workload command and schema1 protocol remain unchanged.
[Expanded coverage](COVERAGE.md) adds all eleven existing range neighbors and
39 cost-focused cases under schema2, with explicit capability outcomes and
mismatch diagnostics. It includes separate stateless workbook-cell, batch and
HyperFormula array-arithmetic routes; none replaces the original per-cell gate.

```sh
python3 benchmarks/expanded.py --sizes 16 64 --samples 3 --warmup 1 \
  --engines poirot-evaluation poirot-overlay ironcalc --output /tmp/poirot-expanded
```

Use a few samples for screening. For a targeted timing difference, use independent
paired process runs, a pilot-informed fixed confirmation count, a two-sided
`p < .03` criterion and a 97% interval on paired log ratios. Stop after at most
ten pairs including pilots, and report inconclusive comparisons without extending
the run. Warmups and inner rounds are not independent evidence.

## Workloads and correctness

| Workload | Shape | Independent expected result |
| --- | --- | --- |
| `arithmetic` | Numeric rows, `A*1.25+3`, final `SUM` | `1.25*n*(n+1)/2 + 3*n`. |
| `fan_out` | One input feeds every row, then `SUM` | Input multiplied by `n*(n+1)/2`. |
| `prefix_sums` | Overlapping growing `SUM` ranges | Row `r` returns `r*(r+1)/2`. |
| `conditional` | Numeric `IF` branches and a final aggregate | Sum the two arithmetic-series branches. |
| `exact_lookup` | Repeated exact `VLOOKUP` over a shared table | Three times the sum of keys, adjusted for edited values. |
| `dependency_chain` | Dependent additions | Input plus chain length; depth capped at 32, below default nesting limits. |
| `independent_branches` | Separate input/formula pairs; edit one branch | Each output is twice its own input. |

`--size` controls breadth (4–5000). Workloads and all initial/edit expectations are
original and deterministic, derived from the arithmetic above rather than any
engine's output. Every output is checked outside timing for every warmup/sample.
An incorrect, nonnumeric, or nonfinite result fails the run. Numeric tolerance is
`1e-9 + 1e-10 * abs(expected)`. A missing/failed engine is an error, never a skip
or a zero-time result. No completed comparison summary is emitted on failure.

## Results and reproducibility

Each run saves raw engine JSON, the full workload with expected results, stderr,
summary JSON/Markdown, and metadata containing hardware, Docker/runtime versions,
resource limits, image IDs, source revision, worktree status/diff fingerprint,
benchmark source hashes, and engine order. Medians and nearest-rank p95 values
are calculated per workload, phase, and edit. Small smoke runs verify the harness;
they are not reliable performance evidence. There is no aggregate winner.

Rust/npm dependencies are locked; Java artifacts and the official LibreOffice
archive have SHA-256 verification. Toolchain/base tags are versioned. Docker
base tags can still move; keep the recorded images to reproduce exact binaries.
Results describe these synthetic workloads on the recorded host and settings,
not general Excel compatibility or every application's speed. Extend the suite
with representative models before making broader comparisons.

Run harness checks independently of Docker:

```sh
python3 -m unittest discover -s benchmarks -p 'test_*.py'
cargo test --locked --manifest-path benchmarks/rust/Cargo.toml --workspace
cargo fmt --manifest-path benchmarks/rust/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path benchmarks/rust/Cargo.toml --workspace --all-targets -- -D warnings
```

## Investigating slow cases

The original `poirot-evaluation` and `poirot-overlay` selections call `.cell()`
for each requested output. `poirot-evaluation-batch` and `poirot-overlay-batch`
request the same ordered outputs in one `.outputs()` call. Both include target
conversion and result collection in their timers; neither skips any formulas.
These workflows publish results at different API boundaries, so report their
identifiers separately.

Use `--diagnostics` to add disjoint four-cell ranges and paired shared ranges
near the top/bottom of a sheet. These distinguish output publication, useful
range traversal and position-dependent discovery work. Validate outputs before
interpreting timings, and use repeated process runs to check whether a difference
persists.

After building the two Java images, exercise their failure paths explicitly:

```sh
POIROT_BENCH_DOCKER_TESTS=1 python3 -m unittest discover -s benchmarks -p 'test_java_adapters.py'
```

These checks require wrong initial/second-edit expectations, text formula results,
formula errors, and blank cells to fail with no timing report, including when
zero could otherwise conceal a nonnumeric result.
A positive sparse-edit check also starts with `A1=1` and `C1=A1+B2`, then creates
`B2=2`; both native evaluators must change the cached result from 1 to 3.

## Persistent scenario replay

The separate [incremental replay harness](incremental-replay/README.md) compares
persistent, reset-per-request and fresh Overlays on generated controls and an
optional external XLSX/JSONL workload. It checks output hashes across all modes,
keeps private values out of reports, and measures first and repeated evaluation
separately with bounded paired statistics. This is a Poirot source comparison,
not a cross-engine XLSX import benchmark.
