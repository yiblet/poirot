/// Select the denominator records for a PIVOTBY PERCENTOF reduction.
/// Each choice expands the current cell to its row, column, parent or grand total.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotRelativeTo {
    /// All row groups within the current column group.
    Column,
    /// All column groups within the current row group.
    Row,
    /// Every admitted record.
    Grand,
    /// The current row group within the parent column group.
    ParentColumn,
    /// The current column group within the parent row group.
    ParentRow,
}
