//! Shared failure domains for adapter policies. Capability details stay typed by family.
use crate::{EvaluationFailure, FunctionFailure};
use poirot_model::CellError;

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

/// Preserve the host's spreadsheet/operational split without touching call order.
pub(crate) fn project_host<Failure>(
    failure: FunctionFailure,
    spreadsheet: impl FnOnce(CellError) -> Failure,
    operational: impl FnOnce(EvaluationFailure) -> Failure,
) -> Failure {
    match failure {
        FunctionFailure::Spreadsheet(error) => spreadsheet(error),
        FunctionFailure::Operational(error) => operational(error),
    }
}

/// Only budget/check portals use this conversion; a cell fault here is a bug.
pub(crate) fn operational_only(failure: FunctionFailure) -> EvaluationFailure {
    match failure {
        FunctionFailure::Operational(error) => error,
        FunctionFailure::Spreadsheet(_) => EvaluationFailure::StalePlan,
    }
}
