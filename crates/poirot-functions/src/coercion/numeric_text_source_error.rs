/// Source parsing contracts that prevent numeric-text evaluation.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum NumericTextSourceError {
    /// Source parsing requires a nonempty separator.
    #[error("source separator is empty")]
    EmptySeparator,
    /// Separator spelling is not a valid source pattern.
    #[error("source separator pattern is invalid")]
    InvalidSeparatorPattern,
    /// Separator needs an unsupported UTF-16 admission contract.
    #[error("source separator unit is unsupported")]
    UnsupportedSeparatorUnit,
}
