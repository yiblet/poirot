//! Merged iteration over effective stored cells.

use super::column_traversal::ColumnTraversal;
use super::input_rows::InputRows;
use super::stored_source::StoredSource;
use crate::model::query::{QueryCounters, RangeMembership};
use crate::model::{Cell, CellAddress};

/// Iterates stored cells after merging baseline content with scenario edits and deletions.
/// Row and column queries visit the effective sparse union without materializing a
/// second membership index; counters include source seeks and prefetched lookups.
#[derive(Clone)]
pub struct EffectiveStoredCells<'a> {
    pub(super) baseline_pages: Option<crate::model::RowPageRange<'a>>,
    pub(super) baseline_rows: Option<crate::model::OccupiedRows<'a>>,
    pub(super) baseline_next_row: Option<u32>,
    pub(super) baseline_sheet: &'a crate::model::Worksheet,
    pub(super) input_rows: Option<InputRows<'a>>,
    pub(super) area: RangeMembership,
    pub(super) reverse: bool,
    pub(super) needs_row: bool,
    pub(super) base_row: Option<crate::model::CellRange<'a>>,
    pub(super) base_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    pub(super) input_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    pub(super) column: Option<ColumnTraversal<'a>>,
    pub(super) counters: QueryCounters,
}

impl<'a> EffectiveStoredCells<'a> {
    /// Return sparse traversal counters accumulated so far.
    pub fn counters(&self) -> QueryCounters {
        self.counters
    }

    /// Step the baseline cursor in its traversal direction.
    pub(super) fn advance_base(
        range: &mut Option<crate::model::CellRange<'a>>,
        reverse: bool,
    ) -> Option<(&'a CellAddress, StoredSource<'a>)> {
        let range = range.as_mut()?;
        let (at, cell) = if reverse {
            range.next_back()
        } else {
            range.next()
        }?;
        Some((at, StoredSource::Baseline(cell)))
    }

    fn advance_input(&mut self, row: u32) -> Option<(&'a CellAddress, StoredSource<'a>)> {
        let inputs = self.input_rows.as_mut()?;
        if self.baseline_rows.is_some() {
            inputs.take_in_row(row)
        } else {
            inputs.take()
        }
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
                    self.base_next = Self::advance_base(&mut self.base_row, self.reverse);
                    self.input_next = self.advance_input(input.0.row);
                    Some(input)
                }
                (Some(base), Some(input)) => {
                    if (base.0 < input.0) != self.reverse {
                        self.base_next = Self::advance_base(&mut self.base_row, self.reverse);
                        Some(base)
                    } else {
                        self.input_next = self.advance_input(input.0.row);
                        Some(input)
                    }
                }
                (Some(base), None) => {
                    self.base_next = Self::advance_base(&mut self.base_row, self.reverse);
                    Some(base)
                }
                (None, Some(input)) => {
                    self.input_next = self.advance_input(input.0.row);
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
            if !self.needs_row {
                return None;
            }
            self.counters.row_seeks += 1;
            let base = self.baseline_next_row;
            let input_row = self.input_rows.as_ref().and_then(InputRows::row);
            let row = match (base, input_row) {
                (Some(a), Some(b)) => {
                    if self.reverse {
                        a.max(b)
                    } else {
                        a.min(b)
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => {
                    self.needs_row = false;
                    return None;
                }
            };
            self.needs_row = if self.reverse {
                row > self.area.first.row
            } else {
                row < self.area.last.row
            };
            self.counters.column_seeks += 1;
            (self.base_row, self.base_next) = if base == Some(row) {
                self.baseline_next_row = self.baseline_rows.as_mut().and_then(|rows| {
                    if self.reverse {
                        rows.next_back()
                    } else {
                        rows.next()
                    }
                });
                let mut range = self
                    .baseline_pages
                    .as_mut()
                    .map(|pages| pages.row(row, self.reverse));
                let selected = Self::advance_base(&mut range, self.reverse);
                (range, selected)
            } else {
                (None, None)
            };
            self.input_next = self.advance_input(row);
        }
    }
}
