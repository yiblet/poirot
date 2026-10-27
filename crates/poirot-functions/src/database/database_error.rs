use crate::scalar::CellError;
/// Separate spreadsheet failures from unsupported criteria/projection behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DatabaseError {
    /// Catchable spreadsheet error produced by selection or aggregation.
    #[error("catchable spreadsheet error produced by selection or aggregation")]
    Cell(CellError),
    /// Formula-based criteria require an unsupported evaluation port.
    #[error("formula-based criteria require an unsupported evaluation port")]
    FormulaCriteria,
    /// Host cannot provide the required text interpretation.
    #[error("host cannot provide the required text interpretation")]
    UnsupportedTextProjection,
}

impl From<CellError> for DatabaseError {
    fn from(value: CellError) -> Self {
        Self::Cell(value)
    }
}
