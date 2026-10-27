//! Failure to start a run over a shared workbook.

use crate::eval::limits::EvaluationError;

/// Failure to start a shared-source run, or an operational failure during setup.
/// Completed setup returns a retained report whose own outcome can still be aborted.
#[derive(Debug, thiserror::Error)]
pub enum SharedRunError {
    /// Input assignments belong to another shared workbook owner.
    #[error("inputs belong to another shared workbook")]
    ForeignInputs,
    /// Evaluation setup or policy validation failed.
    #[error("could not initialize shared evaluation")]
    Evaluation(#[source] EvaluationError),
}

impl From<EvaluationError> for SharedRunError {
    fn from(value: EvaluationError) -> Self {
        Self::Evaluation(value)
    }
}
