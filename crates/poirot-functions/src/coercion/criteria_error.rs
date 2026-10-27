/// Spreadsheet error vocabulary used by criteria matching and aggregation.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CriteriaError {
    /// Empty reference intersection (`#NULL!`).
    #[error("empty criteria intersection")]
    Null,
    /// Zero denominator (`#DIV/0!`).
    #[error("criteria denominator is zero")]
    DivZero,
    /// Invalid criterion or operand (`#VALUE!`).
    #[error("invalid criteria operand")]
    Value,
    /// Invalid reference (`#REF!`).
    #[error("invalid criteria reference")]
    Ref,
    /// Unresolved name (`#NAME?`).
    #[error("unresolved criteria name")]
    Name,
    /// Invalid numeric result (`#NUM!`).
    #[error("invalid criteria number")]
    Num,
    /// Unavailable value (`#N/A`).
    #[error("criteria value unavailable")]
    Na,
}
