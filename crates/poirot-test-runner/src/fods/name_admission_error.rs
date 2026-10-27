/// Failure of conservative original-source non-use proof.
#[derive(Debug, thiserror::Error)]
pub(super) enum NameAdmissionError {
    /// Defined-name inspection limit exceeded.
    #[error("defined-name inspection limit exceeded")]
    NameLimit,
    /// Rejected spelling is outside the ascii identifier proof.
    #[error("rejected spelling is outside the ASCII identifier proof")]
    InvalidNameSpelling,
    /// Worksheet spelling exceeds inspection limit.
    #[error("worksheet spelling exceeds inspection limit")]
    WorksheetNameLimit,
    /// Whole-fixture source inspection limit exceeded.
    #[error("whole-fixture source inspection limit exceeded")]
    SourceSizeLimit,
    /// Formula inspection limit exceeded.
    #[error("formula inspection limit exceeded")]
    FormulaSizeLimit,
    /// Non-ascii source cannot establish non-use.
    #[error("non-ASCII source cannot establish non-use")]
    NonAsciiSource,
    /// Potential rejected-name spelling, including address or string ambiguity.
    #[error("potential rejected-name spelling, including address or string ambiguity")]
    PotentialUse,
    /// Runtime name/formula resolver prevents a static non-use proof.
    #[error("runtime name/formula resolver prevents a static non-use proof")]
    RuntimeResolver,
    /// Source nesting inspection limit exceeded.
    #[error("source nesting inspection limit exceeded")]
    NestingLimit,
    /// Unbalanced source delimiters prevent non-use proof.
    #[error("unbalanced source delimiters prevent non-use proof")]
    UnbalancedDelimiters,
    /// Unrecognized lexical syntax prevents non-use proof.
    #[error("unrecognized lexical syntax prevents non-use proof")]
    UnrecognizedSyntax,
    /// Unterminated source quotation or delimiter prevents non-use proof.
    #[error("unterminated source quotation or delimiter prevents non-use proof")]
    UnterminatedSource,
    /// Source record inspection limit exceeded.
    #[error("source record inspection limit exceeded")]
    RecordLimit,
    /// Complete original xml formula inspection record is unavailable.
    #[error("complete original XML formula inspection record is unavailable")]
    MissingOriginalSources,
    /// Source inspection limit exceeded.
    #[error("source inspection limit exceeded")]
    SourceSizeOverflow,
    /// Inspection.
    #[error("original source at {location} could not establish non-use")]
    Inspection {
        /// Original formula location.
        location: super::inspection_location::InspectionLocation,
        /// Non-use proof failure.
        #[source]
        source: Box<NameAdmissionError>,
    },
    /// Missing formula.
    #[error("original formula source is unavailable at {location}")]
    MissingFormula {
        /// Original formula location.
        location: super::inspection_location::InspectionLocation,
    },
    /// Missing matrix anchor.
    #[error("original matrix-anchor source is unavailable at {location}")]
    MissingMatrixAnchor {
        /// Original formula location.
        location: super::inspection_location::InspectionLocation,
    },
    /// Missing definition.
    #[error("name {name:?}: original definition is unavailable")]
    MissingDefinition {
        /// Defined-name spelling.
        name: String,
    },
}
