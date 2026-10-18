# poirot-test-fods

Run LibreOffice Calc formula checks from the original Flat OpenDocument
Spreadsheet (FODS) XML. The binary reads FODS directly in Rust with `roxmltree`
and evaluates fixtures in parallel with Rayon. Python and an office installation
are not required.

Use the [combined runner](../README.md) for execution, filtering, reports and
baseline comparison. The original source IDs are prefixed with `fods:` in combined
reports. Native source bytes and expectations are unchanged.

The retained baseline now lives in `../baseline.json`. Standalone imported corpora
may still contain native `classifications.json` metadata. The compatibility binary
`cargo run -p poirot-test-runner --bin poirot-test-fods -- ...` remains available for
native harness tooling and uses the same implementation; it is not another crate.

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

See [corpus provenance](../corpus/fods/README.md) and [refresh instructions](fods-REFRESH.md).

`cargo test` covers focused parser, planning, command, reporting and refresh
checks. The full formula corpus runs through the binary rather than a Cargo test
that reparses every fixture.
