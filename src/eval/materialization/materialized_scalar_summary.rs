//! Scalar summaries recorded in publication receipts.

use crate::model::{CellError, CellValue, IeeeNumberBits};

/// Scalar type and numeric/boolean/error identity used in publication receipts.
/// Text is summarized by type rather than copied into each receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterializedScalarSummary {
    /// Blank scalar.
    Blank,
    /// Exact IEEE numeric bits.
    Number(IeeeNumberBits),
    /// Text scalar without copying its contents.
    Text,
    /// Boolean scalar value.
    Boolean(bool),
    /// Spreadsheet error identity.
    Error(CellError),
}

impl MaterializedScalarSummary {
    /// Summarize a scalar's type and non-text identity without allocating.
    pub fn from_value(value: &CellValue) -> Self {
        match value {
            CellValue::Blank => Self::Blank,
            CellValue::Number(n) => Self::Number(IeeeNumberBits::from_number(*n)),
            CellValue::Text(_) | CellValue::TextUtf16(_) => Self::Text,
            CellValue::Boolean(b) => Self::Boolean(*b),
            CellValue::Error(e) => Self::Error(*e),
        }
    }
}
