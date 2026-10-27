//! Separate spreadsheet results from operational aborts.

use super::function_result::FunctionResult;
use super::value::{Value, err};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::model::{CellError, CellValue};

/// Convert an unsupported identifier comparison into an operational capability failure.
pub(in crate::eval) fn identifier_failure(
    error: crate::model::identifiers::IdentifierError,
) -> EvaluationError {
    EvaluationError::UnsupportedCapability {
        capability: CapabilityId::IdentifierComparison,
        target: None,
        reason: crate::eval::limits::EvaluationReasonError::Identifier(std::sync::Arc::new(error)),
    }
}

/// Internal function failure carrier separating catchable errors from aborts.
#[derive(Debug, thiserror::Error)]
pub(in crate::eval) enum FunctionError {
    /// Catchable spreadsheet error.
    #[error("spreadsheet evaluation returned {0}")]
    Spreadsheet(CellError),
    /// Operational failure that formula handlers cannot catch.
    #[error("evaluation stopped operationally")]
    Operational(#[source] EvaluationError),
}

impl From<CellError> for FunctionError {
    fn from(error: CellError) -> Self {
        Self::Spreadsheet(error)
    }
}

impl From<EvaluationError> for FunctionError {
    fn from(failure: EvaluationError) -> Self {
        Self::Operational(failure)
    }
}

/// Restore spreadsheet failures as values while propagating operational aborts.
pub(in crate::eval) fn function_value(result: FunctionResult<CellValue>) -> EvalResult<Value> {
    match result {
        Ok(value) => Ok(value.into()),
        Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionError::Operational(failure)) => Err(failure),
    }
}
