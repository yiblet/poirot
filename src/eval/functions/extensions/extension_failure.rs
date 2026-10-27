use crate::eval::limits::EvaluationFailure;
use crate::model::CellError;

/// Provider failure separated into catchable spreadsheet errors and operational aborts.
/// IFERROR may handle `Spreadsheet`; it cannot handle `Operational`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExtensionFailure {
    /// Catchable semantic error returned by a provider or host coercion.
    Spreadsheet(CellError),
    /// Operational abort that formula error handlers cannot catch.
    Operational(EvaluationFailure),
}

impl From<CellError> for ExtensionFailure {
    fn from(value: CellError) -> Self {
        Self::Spreadsheet(value)
    }
}

impl From<EvaluationFailure> for ExtensionFailure {
    fn from(value: EvaluationFailure) -> Self {
        Self::Operational(value)
    }
}

impl From<crate::eval::values::FunctionFailure> for ExtensionFailure {
    fn from(value: crate::eval::values::FunctionFailure) -> Self {
        match value {
            crate::eval::values::FunctionFailure::Spreadsheet(e) => Self::Spreadsheet(e),
            crate::eval::values::FunctionFailure::Operational(e) => Self::Operational(e),
        }
    }
}
