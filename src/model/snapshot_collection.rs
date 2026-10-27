/// Collection whose record count is bounded during snapshot restoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotCollection {
    /// Workbook worksheets.
    Worksheets,
    /// Workbook defined names.
    DefinedNames,
    /// Stored worksheet cells.
    Cells,
    /// Saved spill rectangles.
    SpillExtents,
    /// Saved spill child records.
    SpillCache,
    /// Explicit worksheet rows.
    Rows,
    /// Row visibility records.
    RowVisibility,
}
