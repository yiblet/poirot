use super::source_convergence_error::SourceConvergenceError;
use crate::scalar::CellError;
/// A statistical calculation failure that retains its cause for the caller.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum StatisticalKernelError<O> {
    /// Catchable spreadsheet error.
    #[error("catchable spreadsheet error")]
    Spreadsheet(CellError),
    /// Host budget or execution failure.
    #[error("host budget or execution failure")]
    Operational(#[source] O),
    /// Pinned source exception requiring explicit boundary handling.
    #[error("pinned source exception requiring explicit boundary handling")]
    SourceConvergence(#[source] SourceConvergenceError),
}
