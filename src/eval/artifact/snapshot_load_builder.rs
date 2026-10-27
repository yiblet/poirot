use super::codec::{decode, read_file};
use super::prepared_policy::PreparedPolicy;
use super::restored_artifact::RestoredArtifact;
use super::snapshot_error::SnapshotError;
use super::snapshot_limits::SnapshotLimits;
use std::path::Path;

/// Configure native snapshot decoding and prepared-state recovery.
///
/// ```
/// use poirot::eval::artifact::{SnapshotSaveBuilder, SnapshotLoadBuilder, PreparedPolicy};
/// use poirot::model::Workbook;
/// let source = Workbook::new();
/// let bytes = SnapshotSaveBuilder::workbook(&source).to_buffer()?;
/// assert!(SnapshotLoadBuilder::default().prepared_policy(PreparedPolicy::RequireCompatible)
///     .from_bytes(&bytes).is_err());
/// let recovered = SnapshotLoadBuilder::default().from_bytes(&bytes)?;
/// assert_eq!(recovered.workbook(), &source);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Default)]
#[must_use]
pub struct SnapshotLoadBuilder {
    pub(super) limits: SnapshotLimits,
    pub(super) policy: PreparedPolicy,
}

impl SnapshotLoadBuilder {
    /// Set resource limits before reading or decoding source.
    pub fn limits(mut self, limits: SnapshotLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Choose whether compatible prepared state may be reused.
    pub fn prepared_policy(mut self, policy: PreparedPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Decode a bounded native snapshot buffer.
    pub fn from_bytes(self, bytes: &[u8]) -> Result<RestoredArtifact, SnapshotError> {
        decode(bytes, self.limits, self.policy)
    }

    /// Read and decode a bounded native snapshot file.
    pub fn open(self, path: impl AsRef<Path>) -> Result<RestoredArtifact, SnapshotError> {
        read_file(path, self.limits, self.policy)
    }
}
