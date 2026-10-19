# poirot-test-runner

The development CLI for authored JSON scenarios and LibreOffice FODS
compatibility fixtures. Runs cases against reviewed baselines and can generate
reports from saved results.

From the repository root:

```sh
cargo run -p poirot-test-runner -- run --source json --filter lookup
cargo run -p poirot-test-runner -- run --source fods --group text
```

Add `--strict` to fail on unchanged known gaps and skips too. Use `list` to preview
case selection. Normal runs do not update baselines. This crate is not published.
