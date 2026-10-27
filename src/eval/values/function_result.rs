use super::function_error::FunctionError;

/// Result of a helper operation with semantic and operational failures preserved.
pub(in crate::eval) type FunctionResult<T> = Result<T, FunctionError>;
