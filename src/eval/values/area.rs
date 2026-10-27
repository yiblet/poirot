use super::area_kind::AreaKind;
use crate::model::CellAddress;

/// A physical worksheet rectangle used for reference reads and spill geometry.
/// Retaining whether it came from a cell or an area lets scalar selection preserve
/// spreadsheet reference semantics even for a one-cell rectangle.
#[derive(Clone, Copy)]
pub(in crate::eval) struct Area {
    pub(in crate::eval) kind: AreaKind,
    pub(in crate::eval) sheet: usize,
    pub(in crate::eval) r0: u32,
    pub(in crate::eval) r1: u32,
    pub(in crate::eval) c0: u32,
    pub(in crate::eval) c1: u32,
}

impl Area {
    pub(in crate::eval) fn contains(self, a: CellAddress) -> bool {
        (self.r0..=self.r1).contains(&a.row) && (self.c0..=self.c1).contains(&a.col)
    }

    pub(in crate::eval) fn size(self) -> u64 {
        u64::from(self.r1 - self.r0 + 1) * u64::from(self.c1 - self.c0 + 1)
    }
}
