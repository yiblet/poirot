use super::snapshot_quota_failure::SnapshotQuotaFailure;
use std::fmt;

/// File-envelope, codec, resource, or semantic restoration failure.
#[derive(Debug)]
pub enum SnapshotError {
    /// Caller-configured file or decoded-resource quota, separate from malformed input.
    Quota(SnapshotQuotaFailure),
    /// Recursive decoder safety guard or an invalid requested nesting ceiling.
    EngineConstraint(&'static str),
    /// A fallible file-buffer allocation failed; no invented byte count is reported.
    Allocation(&'static str),
    /// Filesystem ingress failure.
    Io(std::io::Error),
    /// Unsupported, inconsistent, truncated, or oversized envelope.
    Envelope(&'static str),
    /// Bounded payload encoding or decoding failed.
    Codec(postcard::Error),
    /// Workbook source invariants were invalid.
    Source(crate::model::ModelError),
    /// Prepared state failed the evaluator's semantic validation.
    Workspace(crate::eval::overlay::OverlayError),
}

impl SnapshotError {
    /// Preserve fallible file-buffer allocation failures independently of filesystem failures.
    pub(super) fn from_io(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::OutOfMemory {
            Self::Allocation("snapshot file buffer")
        } else {
            Self::Io(error)
        }
    }
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quota(e) => write!(
                f,
                "snapshot {:?} quota {} exceeded (consumed {}, requested {})",
                e.resource, e.limit, e.consumed, e.requested
            ),
            Self::EngineConstraint(e) => write!(f, "snapshot engine constraint: {e}"),
            Self::Allocation(e) => write!(f, "snapshot allocation refused: {e}"),
            Self::Io(e) => write!(f, "snapshot file: {e}"),
            Self::Envelope(e) => write!(f, "snapshot envelope: {e}"),
            Self::Codec(e) => write!(f, "snapshot codec or decode budget: {e}"),
            Self::Source(e) => write!(f, "snapshot source: {e}"),
            Self::Workspace(e) => write!(f, "snapshot workspace: {e}"),
        }
    }
}

impl std::error::Error for SnapshotError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Codec(e) => Some(e),
            Self::Source(e) => Some(e),
            Self::Workspace(e) => Some(e),
            Self::Envelope(_)
            | Self::Quota(_)
            | Self::EngineConstraint(_)
            | Self::Allocation(_) => None,
        }
    }
}
