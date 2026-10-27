//! Versioned on-disk snapshots with independently recoverable source and optional prepared state.
//!
//! Source schema revisions are independent of engine-prepared compatibility revisions.
//! Unknown source schemas are rejected; incompatible optional prepared state is discarded
//! explicitly, preserving authored edits. These are trusted application artifacts, not an
//! untrusted bytecode interchange format. Decode budgets bound logical values, nesting and
//! copied text/bytes, not allocator-exact peak memory or subsequent engine restoration work.
//!
//! A checkpoint stores the workbook model once, in the source section. Unedited preparation
//! restores against that source, which a CRC-32 at the end of the prepared section
//! identifies; preparation with authored edits also keeps its baseline. Attaching a
//! [`RestoredArtifact`](crate::eval::artifact::RestoredArtifact)'s own workbook skips the baseline comparison that
//! [`WorkspaceState`](crate::eval::overlay::WorkspaceState) applies to caller-supplied ones.
//!
//! ```
//! use poirot::eval::artifact::{SnapshotSaveBuilder, SnapshotLoadBuilder, PreparedStatus};
//! use poirot::model::Workbook;
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_value("A1", 42.)?;
//! let bytes = SnapshotSaveBuilder::workbook(&book).to_buffer()?;
//! let restored = SnapshotLoadBuilder::default().from_bytes(&bytes)?;
//! assert_eq!(restored.prepared_status, PreparedStatus::Absent);
//! assert_eq!(restored.workbook(), &book);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Depends on: overlay (overlay state, snapshots and builders), config (context and
//! policy of restored evaluations) and functions (runtime provider reattachment).

mod bounded;
/// Snapshot file framing, section validation, and bounded serialization.
mod codec;
/// Capped serializer output that rejects oversized snapshots during encoding.
mod output;
/// Whether loading may reuse engine-prepared data or must recover only durable source.
mod prepared_policy;
/// Outcome for the optional prepared section; source is always restored and validated.
mod prepared_status;
/// Validated workbook and optional detached workspace, ready for ordinary evaluation.
mod restored_artifact;
/// Restored preparation or independently recovered source awaiting an explicit policy.
mod restored_checkpoint;
/// Direct calculation restored with an explicit disposition of persisted preparation.
mod restored_evaluation;
mod snapshot_constraint_error;
mod snapshot_envelope_error;
/// File-envelope, codec, resource, or semantic restoration failure.
mod snapshot_error;
/// Optional snapshot loading quotas checked before admission; integer maxima mean no quota.
mod snapshot_limits;
/// Configure native snapshot decoding and prepared-state recovery.
mod snapshot_load_builder;
/// A configured native-snapshot quota refusal with actual decoder admission counts.
mod snapshot_quota_error;
/// Resource admitted during native snapshot loading.
mod snapshot_resource;
/// Configure bounded native snapshot output without recalculating source.
mod snapshot_save_builder;
/// Borrowed source or prepared checkpoint selected for native persistence.
mod snapshot_source;
/// Durable source payload independent of optional evaluator preparation.
mod source_payload;
#[cfg(test)]
mod tests;

pub use codec::{decode, encode_workbook, encode_workspace, read_file};
pub use prepared_policy::PreparedPolicy;
pub use prepared_status::PreparedStatus;
pub use restored_artifact::RestoredArtifact;
pub use restored_checkpoint::RestoredCheckpoint;
pub use restored_evaluation::RestoredEvaluation;
pub use snapshot_error::SnapshotError;
pub use snapshot_limits::SnapshotLimits;
pub use snapshot_load_builder::SnapshotLoadBuilder;
pub use snapshot_quota_error::SnapshotQuotaError;
pub use snapshot_resource::SnapshotResource;
pub use snapshot_save_builder::SnapshotSaveBuilder;

pub use snapshot_envelope_error::SnapshotEnvelopeError;

pub use snapshot_constraint_error::SnapshotConstraintError;
