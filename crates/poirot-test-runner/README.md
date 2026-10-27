# poirot-test-runner

Run spreadsheet compatibility cases and compare results.

The development CLI runs authored JSON scenarios and LibreOffice FODS fixtures
against reviewed baselines. It can also generate reports from saved results.

Run from the repository root:

```sh
cargo run -p poirot-test-runner -- run --source json --filter lookup
cargo run -p poirot-test-runner -- run --source fods --group text
```

Use `list` to preview the selection. Add `--strict` to fail on unchanged known gaps
and skips as well as regressions. Normal runs do not update baselines. This crate
is not published.

Known gaps remain compatibility bugs. Baselines distinguish them from new
regressions so a fix can be reviewed against established behavior. When adding a
case, record the expected Excel result and its evidence; a passing Poirot run
alone does not establish compatibility.

Read each case's observation separately from its baseline change. An unchanged
`mismatch`, `blocked` or `skipped` outcome is not a pass even when the normal run
succeeds. FODS compares LibreOffice fixtures, including source-specific functions
and unsupported fixture contexts; its totals are not an Excel conformance score.
FODS is not a supported application workbook input format.

To retain an inspectable run and generate a report:

```sh
cargo run -p poirot-test-runner -- run --format json > /tmp/poirot-compat-results.jsonl
cargo run -p poirot-test-runner -- report /tmp/poirot-compat-results.jsonl --html /tmp/poirot-compat-report.html
```

The [compatibility guide](../../docs/compatibility.md#run-and-interpret-repository-checks)
explains outcome categories, evidence limits and workbook validation.

Library entry points return module-owned errors: `RunnerError` for combined runs,
`fods::FodsError` for native corpus operations, and `json::JsonError` for authored
scenarios. Match their variants for classification and inspect `Error::source()`
for dependency causes. Saved case diagnostics are report data; they do not replace
these operational errors or spreadsheet error values.
