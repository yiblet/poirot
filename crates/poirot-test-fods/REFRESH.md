# Refresh the FODS corpus

Refresh rebuilds the corpus and its pass/known-gap baseline from a pinned
LibreOffice source tree using the native Rust parser and Rayon evaluator.
Run it from the workspace root:

```sh
cargo run --bin poirot-test-fods -- refresh --source /path/to/pinned/LibreOffice/core
```

The source must identify revision
`cd35bc0879ed5e4b8138b724033ea5937751a84d`: either its Git `HEAD` matches, or a
source archive has a matching `revision.txt`. The fixtures are read from
`sc/qa/unit/data/functions`. No LibreOffice build or Python adapter is needed.

Use `--jobs N` to set the worker count or `--corpus PATH` to select the destination:

```sh
cargo run --bin poirot-test-fods -- refresh \
  --source /path/to/pinned/LibreOffice/core \
  --corpus crates/poirot-test-fods/corpus --jobs 8
```

Refresh copies the original FODS bytes, extracts and evaluates all source cases,
and regenerates `manifest.json` and `classifications.json`. It prepares a complete
replacement before atomically swapping the corpus, preserving its `README.md`
and `COPYING.MPL`. It never rewrites source expectations to match poirot.

Refreshing intentionally changes the baseline: a newly failing case can become
a known gap. Before accepting the refresh, review fixture hashes, source case
counts, omissions, and every classification change. Investigate newly failing
cases instead of accepting them solely because refresh recorded them.

Verify the result:

```sh
cargo test -p poirot-test-fods
cargo run --bin poirot-test-fods
```

A default run attempts known gaps and reports them, but fails only for
regressions or harness errors. `run --strict` additionally fails for known gaps;
use it to inspect unresolved cases, not as an expectation that this baseline
fully passes.

To update the upstream revision, deliberately change `SOURCE_COMMIT` in
`src/fods/mod.rs`, update the provenance revision in `corpus/README.md` and this
document, then refresh from that revision and review and verify the result as
above. Update the baseline counts in the crate README when they change.

To update gap categories while preserving every pass/gap verdict, use
`cargo run --bin poirot-test-fods -- classify` instead of refreshing the baseline.
