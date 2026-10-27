/// Direction in which a logical lookup vector traverses its table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchAxis {
    /// Advance down rows within a fixed column.
    Vertical,
    /// Advance across columns within a fixed row.
    Horizontal,
}
