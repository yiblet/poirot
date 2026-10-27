use super::prepared_status::PreparedStatus;
use crate::eval::overlay::Evaluation;
use crate::model::Workbook;

/// Direct calculation restored with an explicit disposition of persisted preparation.
pub struct RestoredEvaluation {
    /// Direct execution with source and compilation ready to use.
    pub evaluation: Evaluation<'static, Workbook>,
    /// Whether preparation was reused, absent, or discarded.
    pub prepared_status: PreparedStatus,
}
