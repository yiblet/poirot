use crate::model::{CellAddress, ModelError};

/// Rejection of scenario assignments before they can change a workbook.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// A coordinate or source model value was rejected.
    #[error("scenario input model validation rejected")]
    Model(#[from] ModelError),
    /// The assignment counter cannot represent another input.
    #[error("input assignment count overflow")]
    CountOverflow,
    /// Assignment count exceeds the caller's maximum.
    #[error("input count {requested} exceeds limit {limit}")]
    Limit {
        /// Caller-selected maximum assignment count.
        limit: usize,
        /// Number of assignments attempted.
        requested: usize,
    },
    /// More than one assignment targets the same cell.
    #[error("duplicate input assignment at sheet {sheet}, {address:?}")]
    Duplicate {
        /// Worksheet containing both assignments.
        sheet: usize,
        /// Repeated input coordinate.
        address: CellAddress,
    },
}
