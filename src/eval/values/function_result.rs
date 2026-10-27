use super::function_failure::FunctionFailure;

/// Result of a helper operation with semantic and operational failures preserved.
pub(in crate::eval) type FunctionResult<T> = Result<T, FunctionFailure>;
