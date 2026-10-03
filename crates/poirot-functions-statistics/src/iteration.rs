//! Typed iteration failures, separate from spreadsheet numeric-domain errors.
use poirot_functions_core::CellError;

/// Pinned source convergence failures retained separately from spreadsheet and host errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceConvergenceFailure {
    /// The source algorithm exhausted its allowed iterations.
    IterationLimit,
    /// Continued-fraction iteration diverged to infinity.
    InfiniteContinuedFraction,
    /// Continued-fraction iteration produced NaN.
    NanContinuedFraction,
}

/// Preserve spreadsheet, operational, and pinned-source failures across kernel boundaries.
#[derive(Clone, Debug, PartialEq)]
pub enum KernelFailure<O> {
    /// Catchable spreadsheet error.
    Spreadsheet(CellError),
    /// Host budget or execution failure.
    Operational(O),
    /// Pinned source exception requiring explicit boundary handling.
    SourceConvergence(SourceConvergenceFailure),
}

/// Metered statistical result preserving spreadsheet, host, and source-convergence failures.
pub type KernelResult<O> = Result<f64, KernelFailure<O>>;

/// Reserve one iteration through the host-supplied work callback.
pub(crate) fn step<O, M: FnMut(u64) -> Result<(), O>>(
    meter: &mut M,
) -> Result<(), KernelFailure<O>> {
    meter(1).map_err(KernelFailure::Operational)
}

/// Compatibility policy for pure, explicitly unmetered domain entry points.
/// These have no evaluator port; source convergence exceptions map to NUM here.
/// Actual evaluator adapters use the metered entry points and preserve failures.
pub(crate) fn unmetered(result: KernelResult<std::convert::Infallible>) -> crate::Result {
    match result {
        Ok(value) => Ok(value),
        Err(KernelFailure::Spreadsheet(error)) => Err(error),
        Err(KernelFailure::SourceConvergence(_)) => Err(CellError::Num),
        Err(KernelFailure::Operational(never)) => match never {},
    }
}
