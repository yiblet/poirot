//! Keep convergence failures separate from spreadsheet errors and host work limits.
use poirot_functions_core::CellError;

/// Why an iterative statistical calculation failed to converge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceConvergenceFailure {
    /// The source algorithm exhausted its allowed iterations.
    IterationLimit,
    /// Continued-fraction iteration diverged to infinity.
    InfiniteContinuedFraction,
    /// Continued-fraction iteration produced NaN.
    NanContinuedFraction,
}

/// A statistical calculation failure that retains its cause for the caller.
#[derive(Clone, Debug, PartialEq)]
pub enum KernelFailure<O> {
    /// Catchable spreadsheet error.
    Spreadsheet(CellError),
    /// Host budget or execution failure.
    Operational(O),
    /// Pinned source exception requiring explicit boundary handling.
    SourceConvergence(SourceConvergenceFailure),
}

/// A statistical result or its spreadsheet, host, or convergence failure.
pub type KernelResult<O> = Result<f64, KernelFailure<O>>;

/// Charge one iteration to the caller's work counter.
pub(crate) fn step<O, M: FnMut(u64) -> Result<(), O>>(
    meter: &mut M,
) -> Result<(), KernelFailure<O>> {
    meter(1).map_err(KernelFailure::Operational)
}

/// Convert an unmetered kernel result to a spreadsheet result.
///
/// Convergence failures become `#NUM!` here. Evaluator adapters use metered entry
/// points instead, so they can keep convergence and host failures distinct.
pub(crate) fn unmetered(result: KernelResult<std::convert::Infallible>) -> crate::Result {
    match result {
        Ok(value) => Ok(value),
        Err(KernelFailure::Spreadsheet(error)) => Err(error),
        Err(KernelFailure::SourceConvergence(_)) => Err(CellError::Num),
        Err(KernelFailure::Operational(never)) => match never {},
    }
}
