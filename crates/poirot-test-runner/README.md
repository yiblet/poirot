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
