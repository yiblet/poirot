use crate::eval::limits::EvalResult;
use crate::eval::values::{FunctionFailure, FunctionResult, Value, err};

/// Restore semantic failures as cells while preserving operational aborts.
pub(super) fn finish(result: FunctionResult<Value>) -> EvalResult<Value> {
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
