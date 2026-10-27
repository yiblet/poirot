use crate::model::CellAddress;

/// A cell identified by a zero-based workbook sheet index and row/column address.
/// Resolve once with an evaluation's `target` method, including quoted sheet names.
/// Targets bind to that workbook's sheet topology; arbitrary structural edits can invalidate them.
/// A valid unstored coordinate evaluates as blank; a missing worksheet is a resolution error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationTarget {
    /// Zero-based workbook sheet index.
    pub sheet: usize,
    /// Validated row and column coordinates.
    pub address: CellAddress,
}
