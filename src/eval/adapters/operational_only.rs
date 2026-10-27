//! Project mixed function failures at operational-only adapter boundaries.
use crate::eval::limits::EvaluationError;
use crate::eval::values::FunctionError;

/// Project legacy mixed-failure call sites that require an operational abort.
/// Unexpected spreadsheet errors become a stale-plan failure.
pub(in crate::eval) fn operational_only(failure: FunctionError) -> EvaluationError {
    match failure {
        FunctionError::Operational(error) => error,
        FunctionError::Spreadsheet(_) => EvaluationError::StalePlan,
    }
}
