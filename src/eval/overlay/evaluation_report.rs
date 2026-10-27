//! Calculated value with optional detailed profiling.

use crate::eval::limits::EvaluationFailure;

/// Completion with optional profiling; absence is distinct from a zero-work report.
///
/// ```
/// use poirot::eval::{EvaluationContext, overlay::Evaluation};
/// use poirot::model::Workbook;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1".parse()?, "2*3")?;
/// let mut run = Evaluation::with_workbook(&book).build_calculation(EvaluationContext::default())?;
/// run.all()?;
/// let report = run.finish();
/// report.result?;
/// assert!(report.usage.is_none());
/// assert!(report.execution.is_none());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct EvaluationReport<T> {
    /// Calculated result or an operational refusal, never a worksheet error conversion.
    pub result: Result<T, EvaluationFailure>,
    /// Detailed category counters, present only when explicitly requested.
    pub usage: Option<crate::eval::limits::EvaluationUsage>,
    /// Detailed execution/reuse counters, present only when explicitly requested.
    pub execution: Option<crate::eval::limits::RunUsage>,
}

impl<T> EvaluationReport<T> {
    /// Convert a completed internal report without exposing disabled counters as zero work.
    pub(super) fn from_legacy(report: crate::eval::run::RunReport<T>, profiling: bool) -> Self {
        let (result, usage) = match report.outcome {
            crate::eval::limits::EvaluationOutcome::Complete { value, usage } => (Ok(value), usage),
            crate::eval::limits::EvaluationOutcome::Aborted { failure, usage } => {
                (Err(failure), usage)
            }
        };
        Self {
            result,
            usage: profiling.then_some(usage),
            execution: profiling.then_some(report.execution),
        }
    }
}
