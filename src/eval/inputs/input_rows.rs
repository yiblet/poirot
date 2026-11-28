//! Address-ordered traversal of scenario edits inside a worksheet rectangle.

use super::cell_edit::CellEdit;
use super::input_cells::InputCells;
use super::stored_source::StoredSource;
use crate::model::CellAddress;
use crate::model::query::RangeMembership;

/// Retains one edit cursor across rows and seeks past out-of-bounds column runs.
#[derive(Clone)]
pub(super) struct InputRows<'a> {
    cells: &'a InputCells,
    area: RangeMembership,
    reverse: bool,
    remaining: std::collections::btree_map::Range<'a, CellAddress, CellEdit>,
    next: Option<(&'a CellAddress, &'a CellEdit)>,
}

impl<'a> InputRows<'a> {
    /// Open the relevant row span and preserve its first unconsumed edit.
    pub(super) fn new(cells: &'a InputCells, area: RangeMembership, reverse: bool) -> Self {
        let mut remaining = cells.range(
            CellAddress {
                row: area.first.row,
                col: 0,
            }..=CellAddress {
                row: area.last.row,
                col: crate::model::MAX_COLUMNS - 1,
            },
        );
        let next = if reverse {
            remaining.next_back()
        } else {
            remaining.next()
        };
        Self {
            cells,
            area,
            reverse,
            remaining,
            next,
        }
    }

    /// Return the next occupied edit row before clipping its columns.
    pub(super) fn row(&self) -> Option<u32> {
        self.next.map(|(at, _)| at.row)
    }

    /// Take the next edit in address order, used for full-width queries.
    pub(super) fn take(&mut self) -> Option<(&'a CellAddress, StoredSource<'a>)> {
        let (at, edit) = self.next?;
        self.step();
        Some((at, StoredSource::Edit(edit)))
    }

    /// Take an edit inside this row, retaining the first edit of later rows.
    pub(super) fn take_in_row(&mut self, row: u32) -> Option<(&'a CellAddress, StoredSource<'a>)> {
        let (at, _) = self.next.filter(|(at, _)| at.row == row)?;
        let first = self.area.first.col;
        let last = self.area.last.col;
        if (first..=last).contains(&at.col) {
            return self.take();
        }
        if (!self.reverse && at.col < first) || (self.reverse && at.col > last) {
            let col = if self.reverse { last } else { first };
            self.seek(CellAddress { row, col });
        }
        let (at, _) = self.next.filter(|(at, _)| at.row == row)?;
        if (first..=last).contains(&at.col) {
            return self.take();
        }
        // The rest of this row lies outside the rectangle. Skip the run in
        // one seek, even when the scenario changes many columns in the row.
        if let Some(edge) = self.next_row_edge(row) {
            self.seek(edge);
        } else {
            self.next = None;
        }
        None
    }

    fn next_row_edge(&self, row: u32) -> Option<CellAddress> {
        if self.reverse {
            (row > self.area.first.row).then(|| CellAddress {
                // SAFETY: The strict comparison to the inclusive lower row excludes zero.
                row: row - 1,
                col: crate::model::MAX_COLUMNS - 1,
            })
        } else {
            (row < self.area.last.row).then(|| CellAddress {
                // SAFETY: The strict comparison to the inclusive upper row excludes overflow.
                row: row + 1,
                col: 0,
            })
        }
    }

    fn seek(&mut self, edge: CellAddress) {
        let bounds = if self.reverse {
            CellAddress {
                row: self.area.first.row,
                col: 0,
            }..=edge
        } else {
            edge..=CellAddress {
                row: self.area.last.row,
                col: crate::model::MAX_COLUMNS - 1,
            }
        };
        self.remaining = self.cells.range(bounds);
        self.step();
    }

    fn step(&mut self) {
        self.next = if self.reverse {
            self.remaining.next_back()
        } else {
            self.remaining.next()
        };
    }
}
