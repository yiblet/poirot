# Using Poirot

Poirot reads, edits, evaluates and writes Excel workbooks. Start with the
[repository quick start](../README.md#quick-start), then follow the guides below.
These guides describe the public `poirot` crate; component documentation covers
specialized APIs and detailed compatibility contracts.

## Learn the workflow

1. [Create and calculate a workbook](getting-started.md): build a workbook,
   change an input, calculate, apply changes, and save.
2. [Evaluation and overlays](evaluation.md): choose an evaluation mode, configure
   a run, handle failures, and publish changes.
3. [Values, text and formatting](values.md): understand blanks, errors, number
   parsing, exact text, and display formats.

## Use files and repeated calculations

| Task | Guide |
| --- | --- |
| Open and save XLSX or XLS; assess preservation limits | [Excel files](excel-files.md) |
| Reuse results across input changes | [Incremental recalculation](recalculation.md) |
| Persist workbook data and prepared state | [Snapshot files](snapshots.md) |
| Find the right component or advanced API | [Library structure](library-structure.md) |

## Runnable examples

Run these commands from the repository root:

```sh
cargo run --example public_workbook
cargo run --example direct_evaluation
cargo run --example snapshot_file -- /tmp/model.poirot /tmp/result.xlsx
```

The examples cover [XLS/XLSX round trips](../examples/public_workbook.rs),
[a shared workbook and direct evaluation](../examples/direct_evaluation.rs),
and [snapshot restoration](../examples/snapshot_file.rs). The snapshot example
writes to the supplied paths; choose paths you can replace.

For Rust type and method documentation, run
`cargo doc -p poirot --no-deps --open`.
