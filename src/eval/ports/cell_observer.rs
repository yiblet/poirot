use super::cell_completion::CellCompletion;
use crate::eval::limits::EvaluationError;

/// The owner stages bounded version/cache updates here and commits only after the
/// entire run completes. An observer's capacity failure aborts the run; it must
/// discard its transaction on any abort, including failures after earlier cells.
pub trait CellObserver {
    /// Observe a completion; rejection aborts execution operationally.
    fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationError>;
}
