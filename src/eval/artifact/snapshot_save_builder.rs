use super::codec::{encode_workbook, encode_workspace};
use super::snapshot_error::SnapshotError;
use super::snapshot_limits::SnapshotLimits;
use super::snapshot_source::SnapshotSource;
use crate::eval::overlay::snapshot::Snapshot;
use crate::model::Workbook;
use std::path::Path;

/// Configure bounded native snapshot output without recalculating source.
///
/// ```
/// use poirot::eval::artifact::{SnapshotSaveBuilder, SnapshotLoadBuilder, PreparedStatus};
/// use poirot::model::Workbook;
/// let mut source = Workbook::new();
/// source.add_sheet("Inputs")?.set_value("A1".parse()?, 12.)?;
/// let bytes = SnapshotSaveBuilder::workbook(&source).to_buffer()?;
/// let restored = SnapshotLoadBuilder::default().from_bytes(&bytes)?;
/// assert_eq!(restored.prepared_status, PreparedStatus::Absent);
/// assert_eq!(restored.workbook(), &source);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[must_use]
pub struct SnapshotSaveBuilder<'a> {
    pub(super) source: SnapshotSource<'a>,
    pub(super) limits: SnapshotLimits,
}

impl<'a> SnapshotSaveBuilder<'a> {
    /// Persist canonical source and retained file metadata.
    pub fn workbook(workbook: &'a Workbook) -> Self {
        Self {
            source: SnapshotSource::Workbook(workbook),
            limits: SnapshotLimits::for_writing(),
        }
    }

    /// Persist authored edits, preparation, and any retained incremental results.
    pub fn checkpoint(snapshot: Snapshot<'a>) -> Self {
        Self {
            source: SnapshotSource::Checkpoint(snapshot),
            limits: SnapshotLimits::for_writing(),
        }
    }

    /// Set encoding and file resource bounds.
    pub fn limits(mut self, limits: SnapshotLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Serialize the complete snapshot into an owned buffer.
    pub fn to_buffer(self) -> Result<Vec<u8>, SnapshotError> {
        match self.source {
            SnapshotSource::Workbook(w) => encode_workbook(w, self.limits),
            SnapshotSource::Checkpoint(s) => encode_workspace(s, self.limits),
        }
    }

    /// Finish serialization before opening the output file.
    pub fn write_file(self, path: impl AsRef<Path>) -> Result<(), SnapshotError> {
        std::fs::write(path, self.to_buffer()?).map_err(SnapshotError::from_io)
    }
}
