use crate::eval::values::{Area, AreaKind};
use crate::model::CellAddress;

/// An authored formula whose inferred shape could overlap a requested rectangle.
/// Its conservative bounds guide producer scheduling; only successful result
/// admission establishes the cells it actually owns.
#[derive(Clone, Copy)]
pub(in crate::eval) struct ProducerCandidate {
    /// Authored formula coordinate and deterministic scheduling key.
    pub anchor: CellAddress,
    /// Conservative possible footprint clipped to the worksheet grid.
    pub area: Area,
}

impl ProducerCandidate {
    /// Place independent type bounds down/right from a valid authored coordinate.
    pub(super) fn new(sheet: usize, anchor: CellAddress, shape: crate::model::ShapeBounds) -> Self {
        let last = |start: u32, bound: Option<std::num::NonZeroU32>, limit: u32| {
            bound.map_or(limit.saturating_sub(1), |n| {
                start
                    .saturating_add(n.get().saturating_sub(1))
                    .min(limit.saturating_sub(1))
            })
        };
        Self {
            anchor,
            area: Area {
                kind: AreaKind::Area,
                sheet,
                r0: anchor.row,
                c0: anchor.col,
                r1: last(anchor.row, shape.rows, crate::model::MAX_ROWS),
                c1: last(anchor.col, shape.columns, crate::model::MAX_COLUMNS),
            },
        }
    }

    /// Whether a demanded physical rectangle can observe this producer.
    pub(super) fn intersects(self, area: Area) -> bool {
        self.area.sheet == area.sheet
            && self.area.r0 <= area.r1
            && area.r0 <= self.area.r1
            && self.area.c0 <= area.c1
            && area.c0 <= self.area.c1
    }
}
