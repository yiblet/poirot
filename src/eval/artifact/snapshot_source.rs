use crate::eval::overlay::snapshot::Snapshot;
use crate::model::Workbook;

/// Borrowed source or prepared checkpoint selected for native persistence.
#[derive(Clone)]
pub(super) enum SnapshotSource<'a> {
    Workbook(&'a Workbook),
    Checkpoint(Snapshot<'a>),
}
