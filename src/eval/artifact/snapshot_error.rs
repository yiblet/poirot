use super::snapshot_quota_error::SnapshotQuotaError;

/// File-envelope, codec, resource, or semantic restoration failure.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// Caller-configured file or decoded-resource quota, separate from malformed input.
    #[error("snapshot quota exceeded")]
    Quota(#[source] SnapshotQuotaError),
    /// Recursive decoder safety guard or an invalid requested nesting ceiling.
    #[error("snapshot engine constraint")]
    EngineConstraint(#[source] super::snapshot_constraint_error::SnapshotConstraintError),
    /// A fallible file-buffer allocation failed; no invented byte count is reported.
    #[error("could not allocate snapshot file buffer")]
    Allocation(#[source] std::io::Error),
    /// Filesystem ingress failure.
    #[error("could not read snapshot file")]
    Io(#[source] std::io::Error),
    /// Unsupported, inconsistent, truncated, or oversized envelope.
    #[error("snapshot envelope rejected")]
    Envelope(#[source] super::snapshot_envelope_error::SnapshotEnvelopeError),
    /// Bounded payload encoding or decoding failed.
    #[error("snapshot codec rejected payload")]
    Codec(#[source] postcard::Error),
    /// Workbook source invariants were invalid.
    #[error("snapshot workbook source rejected")]
    Source(#[source] crate::model::ModelError),
    /// Prepared state failed the evaluator's semantic validation.
    #[error("snapshot prepared state rejected")]
    Workspace(#[source] crate::eval::overlay::OverlayError),
}

impl SnapshotError {
    /// Preserve fallible file-buffer allocation failures independently of filesystem failures.
    pub(super) fn from_io(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::OutOfMemory {
            Self::Allocation(error)
        } else {
            Self::Io(error)
        }
    }
}
