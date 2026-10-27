use super::evaluation_error::EvaluationError;
use super::evaluation_usage::EvaluationUsage;

/// Completion or operational failure of a calculation, with usage in either case.
/// `Complete` can contain a spreadsheet error value: it means the engine finished
/// evaluating, not that the formula returned a number or text successfully.
#[derive(Clone, Debug)]
pub enum EvaluationOutcome<T> {
    /// Evaluation finished, possibly returning semantic spreadsheet errors.
    Complete {
        /// Completed caller result.
        value: T,
        /// Work consumed by successful execution.
        usage: EvaluationUsage,
    },
    /// Evaluation stopped with an operational failure.
    Aborted {
        /// Failure that stopped execution.
        failure: EvaluationError,
        /// Work consumed before execution stopped.
        usage: EvaluationUsage,
    },
}

impl<T> EvaluationOutcome<T> {
    /// Return usage counters from either a completion or an abort.
    pub fn usage(&self) -> &EvaluationUsage {
        match self {
            Self::Complete { usage, .. } | Self::Aborted { usage, .. } => usage,
        }
    }
}
