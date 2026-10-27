/// An unresolved case mapping between two preserved identities.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unsupported identifier comparison: {stored:?} versus {requested:?}")]
pub struct IdentifierError {
    /// Existing identifier involved in the unsupported comparison.
    pub stored: String,
    /// Requested identifier involved in the unsupported comparison.
    pub requested: String,
}
