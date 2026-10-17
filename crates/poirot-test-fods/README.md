# poirot-test-fods

Run LibreOffice Calc formula checks from the original Flat OpenDocument
Spreadsheet (FODS) XML. The binary reads FODS directly in Rust with `roxmltree`
and evaluates fixtures in parallel with Rayon. Python and an office installation
are not required.

From the workspace root:

```sh
cargo run --bin poirot-test-fods
cargo run --bin poirot-test-fods -- run --jobs 8 --filter text
cargo run --bin poirot-test-fods -- run --jobs 8 --filter text --strict --format json
cargo run --bin poirot-test-fods -- inspect text/fods/trim.fods
cargo run --bin poirot-test-fods -- inspect text/fods/trim.fods --case CASE_ID --format json
cargo run --bin poirot-test-fods -- classify
cargo test -p poirot-test-fods
```

With no subcommand, the binary runs every corpus case. `--filter` selects fixture
names by substring; `--jobs` sets the Rayon worker count. The default corpus is
this crate's `corpus/`; `--corpus PATH` selects another corpus.

Every selected case is attempted, including known gaps. Unsupported workbook
contexts are reported as case failures. Text output streams Rust-style test lines
as cases finish; JSON output streams typed case records followed by a summary.
Reports distinguish
passes, known gaps, regressions and harness errors. The default run fails on
regressions or harness errors; `--strict` also fails on known gaps. JSON output
contains one typed record per line. `inspect` shows the parsed fixture and source
cases without running them; use a reported case ID with `--case` to select one.

Failed lines include a gap type, for example
`test logical__fods__ifs__00000 ... FAILED (known gap: missing_function)`.
Categories distinguish executed missing functions, name resolution, wrong error
types, unexpected/expected errors, numeric/text/boolean/type mismatches,
unsupported syntax, workbook/array contexts, resource limits, and unsupported
checks. Error-code flags or ambiguous predicates retain `predicate_mismatch`;
the runner does not invent an expected error type.

`classify` annotates existing known-gap records with `gap_type` without changing
pass/gap verdicts or accepting regressions. `refresh` rebuilds the observation
baseline from source; see the separate refresh instructions before using it.

The retained baseline has 516 fixtures and 16,114 extracted cases: 1,957 passes
and 14,157 known gaps. Those gaps include 4,450 matrix cases requiring Calc's
array-entry context. They are recorded limitations of the evaluator or harness,
not skipped tests. Original upstream expectations are preserved. Agreement with
Calc is compatibility evidence, not verification against Microsoft Excel.

The COUNTIF text-criterion precision gap is a confirmed POI/LibreOffice
difference; see [the evidence and reproduction](COUNTIF_PRECISION.md). Its
LibreOffice expectation remains intact.

## Structure

Typed structures carry data through parsing, planning, execution and reporting:

- `cli` and `commands`: clap arguments and command orchestration.
- `corpus`: manifest, source hashes and known-gap baseline.
- `fods`, `formula` and `cases`: XML models, OpenFormula translation and extraction
  of the source `Expected`/`Correct` checks.
- `plan` and `execute`: validated fixture plans and Rayon evaluation through ROI.
- `report`: case results, summaries, presentation and run outcome.

Each fixture owns its workbook and one immutable compiled evaluation plan. Each
case uses a fresh default configured evaluation run; its Actual and original
Correct cells share that run's memo and budget. Executed missing function calls
and completed dependent parser failures are reported even when IFERROR or ISERROR
would mask them. Undeclared names produce genuine #NAME errors. Literal and
propagated spreadsheet errors still reach the original predicate. Geometry-only reference inspections and unselected lazy
branches do not count as executed missing capabilities. Diagnostics retain at
most 16 function names, each bounded to 512 characters, plus one unresolved-name
and one parser-failure detail. Expected-only diagnostics do not determine missing
capability failures. Source checker cycles remain visible through shared memo
state.

The original ERROR.TYPE fixture deliberately invokes nonexistent `dhfg()` in F7.
A narrow source annotation preserves that #NAME test: it requires the original
fixture path and digest, exact producer sheet/cell/formula, absent source name
declaration and observed provider consumer. `DHFG` is absent from the pinned
[Calc opcode maps](https://github.com/LibreOffice/core/blob/cd35bc0879ed5e4b8138b724033ea5937751a84d/formula/inc/core_resource.hrc),
and the unchanged F7 source carries Calc #NAME provenance. This does not allow
other providers or replace runtime input with source cached values.

Parsing retains source coordinates and calculation settings, while removing cached formula
values so ROI must calculate the results. Planning checks corpus integrity and
binds cases to the baseline before execution.

See [corpus provenance](corpus/README.md) and [refresh instructions](REFRESH.md).

`cargo test` covers focused parser, planning, command, reporting and refresh
checks. The full formula corpus runs through the binary rather than a Cargo test
that reparses every fixture.
