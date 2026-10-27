/// Numerical admission or execution failure, independent of worksheet errors.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum BesselError<O> {
    /// The argument is outside the finite real domain or the answer overflows.
    #[error("the argument is outside the finite real domain or the answer overflows")]
    Number,
    /// The caller's execution budget refused work.
    #[error("the caller's execution budget refused work")]
    Operational(#[source] O),
    /// The requested order or series exceeds the explicitly bounded kernel.
    #[error("the requested order or series exceeds the explicitly bounded kernel")]
    ComputationalLimit,
}
