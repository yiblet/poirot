use crate::eval::limits::EvaluationError;
use crate::model::CellError;

/// Provider failure separated into catchable spreadsheet errors and operational aborts.
/// IFERROR may handle `Spreadsheet`; it cannot handle `Operational`.
#[derive(Clone, Debug, thiserror::Error)]
pub enum ExtensionError {
    /// Catchable semantic error returned by a provider or host coercion.
    #[error("spreadsheet evaluation returned {0}")]
    Spreadsheet(CellError),
    /// Operational abort that formula error handlers cannot catch.
    #[error("evaluation stopped operationally")]
    Operational(#[source] EvaluationError),
}

impl From<CellError> for ExtensionError {
    fn from(value: CellError) -> Self {
        Self::Spreadsheet(value)
    }
}

impl From<EvaluationError> for ExtensionError {
    fn from(value: EvaluationError) -> Self {
        Self::Operational(value)
    }
}

impl From<crate::eval::values::FunctionError> for ExtensionError {
    fn from(value: crate::eval::values::FunctionError) -> Self {
        match value {
            crate::eval::values::FunctionError::Spreadsheet(e) => Self::Spreadsheet(e),
            crate::eval::values::FunctionError::Operational(e) => Self::Operational(e),
        }
    }
}
