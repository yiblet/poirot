use serde::{Deserialize, Serialize};

/// Where restoration finds the workbook that serialized prepared state was built against.
#[derive(Serialize, Deserialize)]
pub(super) enum SnapshotBaseline<'a> {
    /// Stored in the payload: detached snapshots and checkpoints with authored edits,
    /// whose baseline differs from the effective source saved beside them.
    Model(crate::model::snapshot::Snapshot<'a>),
    /// Supplied on restoration: an unedited checkpoint's source section is its baseline,
    /// so the model is stored only once.
    Source,
}
