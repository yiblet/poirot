# Snapshot files

[Documentation](README.md)

`poirot::artifact` owns a versioned binary file envelope around borrowed
Serde views. Use it for `workbook -> snapshot file -> restore -> evaluate -> XLSX`.
The executable [snapshot_file example](../examples/snapshot_file.rs) demonstrates
that workflow, including an actual snapshot file and XLSX reopening:

```sh
cargo run --example snapshot_file -- /tmp/model.poirot /tmp/result.xlsx
```

## Choose what to persist

Choose `encode_workbook` for source-only persistence. Choose `encode_workspace`
for direct or overlay preparation. Both accept import warnings explicitly; pass
`Spreadsheet::import_warnings()` so import diagnostics survive storage. Write the
returned bytes using the application's file-publication policy. `read_file` reads
under a configured size bound; `decode` accepts existing bytes under the same bound.

## Restore and handle compatibility

The fixed header is inspected before payload decoding. It identifies the file
format version, source schema, source-only versus prepared kind, engine preparation
compatibility revision, and exact section lengths. Unknown source schemas and file
versions fail. Only source schema 1 is supported; other source schemas are rejected. Raw Serde snapshots remain
available for applications that already own their encoding and compatibility policy.

Source and prepared data are separate sections. Prepared files retain an independent
effective source, including authored cell edits and deletions, so rejecting compiled
state cannot silently lose those edits. Computed values are not promoted to input
literals. Compatible preparation must agree exactly with its effective source.
`PreparedPolicy::ReuseCompatible` restores compatible state and explicitly reports
`DiscardedIncompatible` otherwise. `SourceOnly` skips prepared decoding, useful when
providers are unavailable or the caller wants fresh preparation. `RequireCompatible`
rejects mismatched engine revisions. Compatible but malformed preparation fails;
it is not silently treated as incompatible. Discarding preparation also discards its
execution policies: choose an explicit workspace policy for the recovered source.

`RestoredArtifact::workbook` is the exact baseline for its optional `workspace`.
Attach that state with the existing borrowed/Arc and extension-aware APIs, or use
`into_evaluation` to restore direct execution with a fresh run context. If no state
was retained, `workbook` is already the effective authored source. Provider code is
never serialized; missing/mismatched registries still fail at attachment. A restored
overlay may retain validated results; a restored direct evaluation starts fresh.

## Limits and trust

`SnapshotLimits` bounds total file bytes, cumulative decoded typed values, copied
string/byte payload, and nested decode depth. These apply across both sections.
Collection length claims are checked before visitors can reserve memory, and
untrusted collection hints are never passed to allocating visitors. Strings and
bytes are checked before Postcard's borrowed payload reaches the owning visitor.
Encoding also caps its output as bytes are produced. The limits do not claim an
allocator-exact process memory bound; workbook construction and cache rebuilding
have their own validation and allocation costs. Prepared snapshots remain trusted
application state, not an untrusted bytecode interchange format or an authenticated
file. The envelope detects structural errors, not arbitrary corruption or tampering.

## Export after restoring

Calculation and file export remain separate. `all()` refreshes every modeled stored
cell and discovered spill child; selected `outputs()` does not refresh unrelated
formula caches. Saving preserves caches and never recalculates. Preserve spreadsheet
errors as values and propagate operational failures before output publication. XLSX
output reconstructs supported workbook data and does not recover omitted charts,
macros, rich styles, or other features described by import warnings. Direct writes
are not atomic filesystem replacement; applications choose publication/durability
requirements for both snapshot and XLSX files.

## Raw Serde snapshots

For applications that supply their own encoding, compression, storage, compatibility
envelope and decode limits, `snapshot()` returns a borrowed Serde view. `Workbook`,
`Overlay` and `Evaluation` do not implement Serde themselves. This example uses
Postcard to round-trip a workbook and an overlay:

```rust
use poirot::{Overlay, Workbook};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut workbook = Workbook::new();
    workbook.add_sheet("S")?;
    let bytes = postcard::to_allocvec(&workbook.snapshot())?;
    let snapshot: poirot::WorkbookSnapshot<'static> = postcard::from_bytes(&bytes)?;
    let restored = Workbook::from_snapshot(snapshot)?;

    let overlay = Overlay::with_workbook(&restored).build()?;
    let bytes = postcard::to_allocvec(&overlay.snapshot())?;
    let snapshot: poirot::OverlaySnapshot<'static> = postcard::from_bytes(&bytes)?;
    let (restored, state) = Overlay::from_snapshot(snapshot)?;
    let _overlay = state.attach(&restored)?;
    Ok(())
}
```

An overlay snapshot retains the baseline workbook, edits, compiled formulas,
results, dependencies and spill ownership. Restoration rebuilds runtime indexes
without parsing stored compiled formulas. Numeric bits and exact UTF-16 units
survive round trips. Use `into_owned()` to detach a snapshot from its owner.
Attachment requires an exactly matching baseline, including source metadata.
Runtime providers must be reattached separately. These raw snapshots are trusted,
same-engine application state; apply your own compatibility and decoder bounds.
