use super::prefix_interval::PrefixInterval;
use super::prefix_partition::PrefixPartition;
use crate::model::CellAddress;

/// A physical lookup interval used as the identity of a reusable search prefix.
/// Sheet and cell endpoints keep cache reuse tied to the original worksheet geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::eval) struct PrefixAxis {
    pub(in crate::eval) sheet: usize,
    pub(in crate::eval) first: CellAddress,
    pub(in crate::eval) last: CellAddress,
}

impl PrefixAxis {
    pub(in crate::eval) fn location(self) -> (PrefixPartition, PrefixInterval) {
        if self.first.col == self.last.col {
            (
                (self.sheet, true, self.first.col),
                (self.first.row, self.last.row),
            )
        } else {
            (
                (self.sheet, false, self.first.row),
                (self.first.col, self.last.col),
            )
        }
    }

    pub(in crate::eval) fn from_location(
        (sheet, vertical, fixed): PrefixPartition,
        (first, last): PrefixInterval,
    ) -> Self {
        let address = |n| {
            if vertical {
                CellAddress { row: n, col: fixed }
            } else {
                CellAddress { row: fixed, col: n }
            }
        };
        Self {
            sheet,
            first: address(first),
            last: address(last),
        }
    }
}
