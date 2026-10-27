/// Failure of Excel specification validation or execution.
#[derive(Debug, thiserror::Error)]
pub enum SpecificationError {
    /// Nonrectangular array footprint.
    #[error("nonrectangular array footprint")]
    NonrectangularFootprint,
    /// Documented assertion has no expected result.
    #[error("documented assertion has no expected result")]
    MissingExpectedResult,
    /// Empty array footprint.
    #[error("empty array footprint")]
    EmptyFootprint,
    /// Footprint row overflow.
    #[error("footprint row overflow")]
    FootprintRowOverflow,
    /// Footprint column overflow.
    #[error("footprint column overflow")]
    FootprintColumnOverflow,
    /// Specification step has no assertions.
    #[error("specification step has no assertions")]
    MissingAssertions,
    /// Invalid specification tolerance.
    #[error("invalid specification tolerance")]
    InvalidTolerance,
    /// Specification assertion lacks evidence.
    #[error("specification assertion lacks evidence")]
    MissingEvidence,
    /// Excel specification inventory differs from the public registry.
    #[error("Excel specification inventory differs from the public registry")]
    InventoryMismatch,
    /// At least 1,000 excel specifications are required.
    #[error("at least 1,000 Excel specifications are required")]
    InsufficientCases,
    /// Fixture worksheet is missing.
    #[error("fixture worksheet is missing")]
    MissingWorksheet,
    /// Specification artifact i/o failed.
    #[error("specification artifact I/O failed")]
    Io(#[from] std::io::Error),
    /// Specification json decoding failed.
    #[error("specification JSON decoding failed")]
    Json(#[from] serde_json::Error),
    /// Specification workbook mutation failed.
    #[error("specification workbook mutation failed")]
    Model(#[from] poirot::model::ModelError),
    /// Specification evaluation setup failed.
    #[error("specification evaluation setup failed")]
    Overlay(#[from] poirot::eval::overlay::OverlayError),
    /// Specification evaluation failed.
    #[error("specification evaluation failed")]
    Evaluation(#[from] poirot::eval::EvaluationError),
    /// Specification coordinate exceeds representation.
    #[error("specification coordinate exceeds representation")]
    Coordinate(#[from] std::num::TryFromIntError),
    /// Cell must contain exactly one formula or typed value.
    #[error("cell must contain exactly one formula or typed value")]
    InvalidCellContent,
    /// Failed cases.
    #[error("Excel specification cases failed: {cases:?}")]
    FailedCases {
        /// Names of cases with assertion or execution failures.
        cases: Vec<String>,
    },
    /// Invalid identity.
    #[error("invalid Excel specification identity {name:?}")]
    InvalidIdentity {
        /// Case name.
        name: String,
    },
}
