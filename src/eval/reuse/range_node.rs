use super::key::Key;
use crate::model::CellAddress;
use crate::model::query::RangeMembership;

/// One read rectangle and its subtree's enclosing worksheet coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RangeNode {
    /// Inclusive corners of the consumer's rectangle on this tree's sheet.
    pub(super) start: CellAddress,
    pub(super) end: CellAddress,
    pub(super) reader: Key,
    pub(super) first: CellAddress,
    pub(super) last: CellAddress,
}

impl RangeNode {
    /// Initialize a rectangle's bounds before aggregating its descendants.
    pub(super) fn new(range: RangeMembership, reader: Key) -> Self {
        Self {
            first: range.first,
            last: range.last,
            start: range.first,
            end: range.last,
            reader,
        }
    }

    /// Expand this subtree to cover another subtree on the same sheet.
    pub(super) fn enclose(&mut self, first: CellAddress, last: CellAddress) {
        self.first.row = self.first.row.min(first.row);
        self.first.col = self.first.col.min(first.col);
        self.last.row = self.last.row.max(last.row);
        self.last.col = self.last.col.max(last.col);
    }
}
