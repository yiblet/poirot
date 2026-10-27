#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
/// Classification needs the full parser, either for invalid input or syntax
/// outside this fast path. This is not itself a formula parsing diagnostic.
pub enum ClassificationError {
    /// Syntax requires the full parser rather than token classification.
    #[error("syntax requires full parsing")]
    Syntax,
    /// Reference syntax cannot be safely classified by this fast path.
    #[error("reference requires full parsing")]
    Reference,
    /// Range endpoints are not a supported bounded cell pair.
    #[error("range requires full parsing")]
    Range,
    /// Sheet qualification requires the full parser.
    #[error("qualified reference requires full parsing")]
    QualifiedReference,
    /// Array or error literal requires full syntax parsing.
    #[error("array or error literal requires full parsing")]
    ArrayOrError,
}
