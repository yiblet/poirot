/// Why an iterative statistical calculation failed to converge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SourceConvergenceError {
    /// The source algorithm exhausted its allowed iterations.
    #[error("the source algorithm exhausted its allowed iterations")]
    IterationLimit,
    /// Continued-fraction iteration diverged to infinity.
    #[error("continued-fraction iteration diverged to infinity")]
    InfiniteContinuedFraction,
    /// Continued-fraction iteration produced NaN.
    #[error("continued-fraction iteration produced nan")]
    NanContinuedFraction,
}
