/// Invalid scalar spelling. Formula errors remain ordinary cell values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValueError {
    /// A spelling does not identify a supported spreadsheet error value.
    #[error("unknown spreadsheet error spelling {input:?}")]
    UnknownCellError {
        /// Rejected spreadsheet error spelling.
        input: String,
    },
}

impl From<std::convert::Infallible> for ValueError {
    /// Admit infallible typed conversions into checked model operations.
    fn from(value: std::convert::Infallible) -> Self {
        match value {}
    }
}
