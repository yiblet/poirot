use super::evaluation_failure::EvaluationFailure;

/// Internal result carrier that keeps operational failures separate from cell errors.
pub(in crate::eval) type EvalResult<T> = Result<T, EvaluationFailure>;
