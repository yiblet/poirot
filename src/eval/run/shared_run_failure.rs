//! Failure to start a run over a shared workbook.

use crate::eval::limits::EvaluationFailure;

/// Failure to start a shared-source run, or an operational failure during setup.
/// Completed setup returns a retained report whose own outcome can still be aborted.
#[derive(Debug)]
pub enum SharedRunFailure {
    /// Input assignments belong to another shared workbook owner.
    ForeignInputs,
    /// Evaluation setup or policy validation failed.
    Evaluation(EvaluationFailure),
}

impl From<EvaluationFailure> for SharedRunFailure {
    fn from(value: EvaluationFailure) -> Self {
        Self::Evaluation(value)
    }
}
