/// Spreadsheet error vocabulary shared by lookup kernels and host adapters.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LookupValueError {
    /// Invalid operand or argument shape (`#VALUE!`).
    #[error("invalid lookup operand")]
    Value,
    /// Invalid reference (`#REF!`).
    #[error("invalid lookup reference")]
    Ref,
    /// No matching value (`#N/A`).
    #[error("lookup value unavailable")]
    Na,
    /// Invalid numeric argument or result (`#NUM!`).
    #[error("invalid lookup number")]
    Num,
    /// Zero denominator (`#DIV/0!`).
    #[error("lookup denominator is zero")]
    Div0,
    /// Unresolved name (`#NAME?`).
    #[error("unresolved lookup name")]
    Name,
    /// Empty intersection (`#NULL!`).
    #[error("empty lookup intersection")]
    Null,
}
