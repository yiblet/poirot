//! Keep convergence failures separate from spreadsheet errors and host work limits.
use super::statistical_kernel_error::StatisticalKernelError;
use crate::scalar::CellError;

/// A statistical result or its spreadsheet, host, or convergence failure.
pub type KernelResult<O> = Result<f64, StatisticalKernelError<O>>;

/// Charge one iteration to the caller's work counter.
pub(crate) fn step<O, M: FnMut(u64) -> Result<(), O>>(
    meter: &mut M,
) -> Result<(), StatisticalKernelError<O>> {
    meter(1).map_err(StatisticalKernelError::Operational)
}

/// Convert an unmetered kernel result to a spreadsheet result.
///
/// Convergence failures become `#NUM!` here. Evaluator adapters use metered entry
/// points instead, so they can keep convergence and host failures distinct.
pub(crate) fn unmetered(
    result: KernelResult<std::convert::Infallible>,
) -> crate::statistics::Result {
    match result {
        Ok(value) => Ok(value),
        Err(StatisticalKernelError::Spreadsheet(error)) => Err(error),
        Err(StatisticalKernelError::SourceConvergence(_)) => Err(CellError::Num),
        Err(StatisticalKernelError::Operational(never)) => match never {},
    }
}
