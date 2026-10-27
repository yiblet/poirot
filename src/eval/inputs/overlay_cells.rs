//! Merged base and overlay source iteration.

use super::input_cells::InputCells;
use super::overlay_range::OverlayRange;
use crate::model::CellAddress;

/// Merged base/overlay source iterator preserving edit precedence.
#[derive(Clone, Copy)]
pub(super) enum OverlayCells<'a> {
    Base(crate::model::WorksheetCells<'a>),
    Input(&'a InputCells),
}

impl<'a> OverlayCells<'a> {
    /// Restrict the source to a coordinate range.
    pub(super) fn range(self, range: std::ops::RangeInclusive<CellAddress>) -> OverlayRange<'a> {
        match self {
            Self::Base(c) => OverlayRange::Base(c.range(range)),
            Self::Input(c) => OverlayRange::Input(c.range(range)),
        }
    }
}
