//! Merged iteration over effective stored cells.

use super::column_traversal::ColumnTraversal;
use super::input_cells::InputCells;
use super::overlay_cells::OverlayCells;
use super::overlay_range::{OverlayRange, advance};
use super::stored_source::StoredSource;
use crate::model::query::{QueryCounters, RangeMembership};
use crate::model::{Cell, CellAddress};

/// Iterates stored cells after merging baseline content with scenario edits and deletions.
/// Row and column queries visit the effective sparse union without materializing a
/// second membership index; counters include source seeks and prefetched lookups.
pub struct EffectiveStoredCells<'a> {
    pub(super) baseline: crate::model::WorksheetCells<'a>,
    pub(super) baseline_sheet: &'a crate::model::Worksheet,
    pub(super) overlay: Option<&'a InputCells>,
    pub(super) area: RangeMembership,
    pub(super) reverse: bool,
    pub(super) next_row: Option<u32>,
    pub(super) base_row: Option<OverlayRange<'a>>,
    pub(super) input_row: Option<OverlayRange<'a>>,
    pub(super) base_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    pub(super) input_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    pub(super) column: Option<ColumnTraversal<'a>>,
    pub(super) counters: QueryCounters,
}

impl EffectiveStoredCells<'_> {
    /// Return sparse traversal counters accumulated so far.
    pub fn counters(&self) -> QueryCounters {
        self.counters
    }
}

impl<'a> Iterator for EffectiveStoredCells<'a> {
    /// Borrowed effective source cell excluding removed baseline entries.
    type Item = (&'a CellAddress, &'a Cell);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(column) = &mut self.column {
            loop {
                let item = column.next(&mut self.counters)?;
                let Some(cell) = item.1.cell() else { continue };
                if matches!(item.1, StoredSource::Edit(_))
                    || self.baseline_sheet.spill_owner(*item.0).is_none()
                {
                    return Some((item.0, cell));
                }
                if let Some(source) = self.baseline_sheet.source_cell(*item.0) {
                    return Some((item.0, source));
                }
            }
        }
        loop {
            let selected = match (self.base_next, self.input_next) {
                (Some(base), Some(input)) if base.0 == input.0 => {
                    self.base_next = advance(&mut self.base_row, self.reverse);
                    self.input_next = advance(&mut self.input_row, self.reverse);
                    Some(input)
                }
                (Some(base), Some(input)) => {
                    if (base.0 < input.0) != self.reverse {
                        self.base_next = advance(&mut self.base_row, self.reverse);
                        Some(base)
                    } else {
                        self.input_next = advance(&mut self.input_row, self.reverse);
                        Some(input)
                    }
                }
                (Some(base), None) => {
                    self.base_next = advance(&mut self.base_row, self.reverse);
                    Some(base)
                }
                (None, Some(input)) => {
                    self.input_next = advance(&mut self.input_row, self.reverse);
                    Some(input)
                }
                (None, None) => None,
            };
            if let Some(item) = selected {
                let Some(cell) = item.1.cell() else { continue };
                let item = if matches!(item.1, StoredSource::Edit(_))
                    || self.baseline_sheet.spill_owner(*item.0).is_none()
                {
                    (item.0, cell)
                } else if let Some(source) = self.baseline_sheet.source_cell(*item.0) {
                    (item.0, source)
                } else {
                    continue;
                };
                self.counters.cells_visited += 1;
                return Some(item);
            }
            let next = self.next_row?;
            self.counters.row_seeks += 1;
            let first = CellAddress {
                row: if self.reverse {
                    self.area.first.row
                } else {
                    next
                },
                col: 0,
            };
            let last = CellAddress {
                row: if self.reverse {
                    next
                } else {
                    self.area.last.row
                },
                col: crate::model::MAX_COLUMNS - 1,
            };
            let row_in = |cells: OverlayCells<'a>| {
                let mut range = cells.range(first..=last);
                if self.reverse {
                    range.next_back()
                } else {
                    range.next()
                }
            };
            let base = row_in(OverlayCells::Base(self.baseline));
            let input = self
                .overlay
                .and_then(|cells| row_in(OverlayCells::Input(cells)));
            let row = match (base.map(|(a, _)| a.row), input.map(|(a, _)| a.row)) {
                (Some(a), Some(b)) => {
                    if self.reverse {
                        a.max(b)
                    } else {
                        a.min(b)
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => {
                    self.next_row = None;
                    return None;
                }
            };
            self.next_row = if self.reverse {
                (row > self.area.first.row).then(|| row - 1)
            } else {
                (row < self.area.last.row).then_some(row + 1)
            };
            self.counters.column_seeks += 1;
            let first = CellAddress {
                row,
                col: self.area.first.col,
            };
            let last = CellAddress {
                row,
                col: self.area.last.col,
            };
            let prepare_row =
                |cells: OverlayCells<'a>, selected: Option<(&'a CellAddress, StoredSource<'a>)>| {
                    // A one-column query can use the cell that located its row.
                    // Reopening a singleton range would seek the same B-tree twice.
                    if first == last && selected.is_some_and(|(address, _)| *address == first) {
                        (None, selected)
                    } else {
                        let mut range = Some(cells.range(first..=last));
                        let selected = advance(&mut range, self.reverse);
                        (range, selected)
                    }
                };
            (self.base_row, self.base_next) = prepare_row(OverlayCells::Base(self.baseline), base);
            (self.input_row, self.input_next) = self.overlay.map_or((None, None), |cells| {
                prepare_row(OverlayCells::Input(cells), input)
            });
        }
    }
}
