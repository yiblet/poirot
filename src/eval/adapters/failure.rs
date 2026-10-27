//! Shared failure domains for adapter policies. Capability details stay typed by family.
use crate::eval::limits::EvaluationFailure;
use crate::eval::values::FunctionFailure;
use crate::model::CellError;

/// Shared semantic and operational failures with typed family capability details.
#[derive(Clone, Debug, PartialEq)]
pub enum AdapterFailure<Operational, Capability> {
    /// Catchable spreadsheet error.
    Spreadsheet(CellError),
    /// Host operational abort.
    Operational(Operational),
    /// Family-specific host capability is unavailable.
    UnsupportedCapability(Capability),
}

impl<Operational, Capability> From<CellError> for AdapterFailure<Operational, Capability> {
    fn from(error: CellError) -> Self {
        Self::Spreadsheet(error)
    }
}

/// Project legacy mixed-failure call sites that require an operational abort.
/// Unexpected spreadsheet errors become a stale-plan failure.
pub(in crate::eval) fn operational_only(failure: FunctionFailure) -> EvaluationFailure {
    match failure {
        FunctionFailure::Operational(error) => error,
        FunctionFailure::Spreadsheet(_) => EvaluationFailure::StalePlan,
    }
}
