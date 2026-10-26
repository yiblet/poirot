//! Separate spreadsheet results from operational aborts.
use super::*;

/// Convert an unsupported identifier comparison into an operational capability failure.
pub(super) fn identifier_failure(
    error: crate::model::identifiers::IdentifierError,
) -> EvaluationFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::IdentifierComparison,
        source: None,
        detail: error.to_string(),
    }
}

/// Internal function failure carrier separating catchable errors from aborts.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum FunctionFailure {
    /// Catchable spreadsheet error.
    Spreadsheet(CellError),
    /// Operational failure that formula handlers cannot catch.
    Operational(EvaluationFailure),
}
impl From<CellError> for FunctionFailure {
    fn from(error: CellError) -> Self {
        Self::Spreadsheet(error)
    }
}
impl From<EvaluationFailure> for FunctionFailure {
    fn from(failure: EvaluationFailure) -> Self {
        Self::Operational(failure)
    }
}
/// Result of a helper operation with semantic and operational failures preserved.
pub(super) type FunctionResult<T> = Result<T, FunctionFailure>;
/// Restore spreadsheet failures as values while propagating operational aborts.
pub(super) fn function_value(result: FunctionResult<CellValue>) -> EvalResult<Value> {
    match result {
        Ok(value) => Ok(value.into()),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(failure)) => Err(failure),
    }
}
