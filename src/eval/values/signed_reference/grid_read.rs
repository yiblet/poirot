use crate::model::CellAddress;

/// Physical projection of a logical reference read.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::eval) enum GridRead {
    /// Off-grid logical cell, yielding a reference-origin blank.
    Blank,
    /// In-grid workbook cell to read normally.
    Cell {
        /// Workbook sheet index.
        sheet: usize,
        /// Physical cell coordinates.
        address: CellAddress,
    },
}
