/// Rejection of a local sheet-qualified evaluation target.
#[derive(Debug, thiserror::Error)]
pub enum OverlayTargetError {
    /// Input is not one local sheet-qualified cell.
    #[error("target must be one local sheet-qualified cell")]
    InvalidReference,
    /// Cell coordinate is outside worksheet bounds.
    #[error("target coordinate rejected")]
    Coordinate(#[from] poirot_io::ModelError),
    /// Quoted sheet syntax could not be parsed.
    #[error("target sheet syntax rejected")]
    Syntax(#[from] poirot_parsing::formula::ParseError),
    /// Comparison requires unsupported identifier behavior.
    #[error("target worksheet comparison rejected")]
    Identifier(#[from] crate::model::identifiers::IdentifierError),
    /// No worksheet has the requested spelling.
    #[error("target worksheet {name:?} is absent")]
    MissingWorksheet {
        /// Requested worksheet spelling.
        name: String,
    },
}
