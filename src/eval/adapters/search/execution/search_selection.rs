/// Zero-based position selected within the logical lookup vector, not an absolute worksheet row/column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchSelection {
    /// Zero-based logical position in the lookup vector.
    pub index: usize,
}
