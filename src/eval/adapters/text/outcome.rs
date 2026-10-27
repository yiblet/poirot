use crate::eval::limits::EvalResult;
use crate::eval::values::{FunctionError, FunctionResult, Value, err};

/// Restore semantic failures as cells while preserving operational aborts.
pub(super) fn finish(result: FunctionResult<Value>) -> EvalResult<Value> {
    match result {
        Ok(value) => Ok(value),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(error)) => Err(error),
    }
}
