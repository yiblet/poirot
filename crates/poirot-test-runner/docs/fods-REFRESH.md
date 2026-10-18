# Refreshing pinned FODS sources

The checked-in corpus uses the combined runner's `baseline.json`. Source extraction
and acceptance of observed results are separate operations.

To extract a standalone corpus from the pinned LibreOffice checkout:

```sh
cargo run -p poirot-test-runner -- fods refresh \
  --source /path/to/pinned/LibreOffice/core \
  --corpus /tmp/poirot-fods-import --jobs 8
```

The source must match the pinned revision and have no local fixture edits. The
extractor retains native fixture bytes, licensing, source hashes, original
expectations and predicates. It stages output and preserves the destination on
failure. It refuses directories containing unrelated files. The combined CLI
requires a separate destination instead of overwriting the checked-in corpus.

Standalone output includes `manifest.json`, native `classifications.json`,
`fixtures/`, and licensing. Native classification exists for import review and
historical tooling; it is not a second baseline for the checked-in corpus.

Review imported bytes, hashes, case IDs and provenance before replacing source
assets. Keep the checked-in shared baseline for comparison: new cases appear as
new tests, and removed IDs appear in full-run comparisons. Do not copy the import's
`classifications.json` into the checked-in corpus.

After reviewing the import, run both default corpora and generate an explicit
baseline candidate as described in the [combined runner guide](../README.md).
Existing native baseline verdicts must not be silently refreshed to waive failures.

The compatibility binary remains available to inspect or classify a standalone
import using its historical metadata format:

```sh
cargo run -p poirot-test-runner --bin poirot-test-fods -- \
  classify --corpus /tmp/poirot-fods-import
```

Native `classify` changes category labels only, not pass/gap verdicts. Neither
baseline records nor Calc results establish Microsoft Excel compatibility.
