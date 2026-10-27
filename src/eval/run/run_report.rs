use crate::eval::limits::EvaluationOutcome;
use crate::eval::limits::RunUsage;

/// A calculation outcome and its execution counters.
/// Inspect `outcome` for operational failure; usage alone does not establish success.
#[derive(Clone, Debug)]
pub struct RunReport<T> {
    /// Caller result or operational abort, including metered usage.
    pub outcome: EvaluationOutcome<T>,
    /// Execution and reuse counters for this run.
    pub execution: RunUsage,
}
