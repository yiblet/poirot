use super::prepared_status::PreparedStatus;
use crate::eval::overlay::Overlay;
use crate::model::Workbook;

/// Restored preparation or independently recovered source awaiting an explicit policy.
pub enum RestoredCheckpoint {
    /// Validated preparation attached to its owned original baseline.
    Prepared(Box<Overlay<'static, Workbook>>),
    /// Durable source recovered after preparation was absent or discarded.
    Source {
        /// Independently recovered authored source and file metadata.
        workbook: Workbook,
        /// Why preparation was absent or discarded.
        status: PreparedStatus,
    },
}
