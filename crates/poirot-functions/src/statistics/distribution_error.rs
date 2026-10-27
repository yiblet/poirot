use crate::scalar::CellError;
/// DistributionError retaining the distinction between mathematical errors and execution limits.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum DistributionError<O> {
    /// Invalid worksheet domain or nonrepresentable mathematical result.
    #[error("invalid worksheet domain or nonrepresentable mathematical result")]
    Spreadsheet(CellError),
    /// Caller work or execution failure.
    #[error("caller work or execution failure")]
    Operational(#[source] O),
    /// A finite numerical iteration did not meet its convergence criterion.
    #[error("a finite numerical iteration did not meet its convergence criterion")]
    NonConvergence,
    /// Requested work exceeds the numerical implementation's finite allowance.
    #[error("requested work exceeds the numerical implementation's finite allowance")]
    ComputationalLimit,
}
