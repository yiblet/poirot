//! Column-ordered cursor over authoritative edits.

use super::cell_edit::CellEdit;
use super::input_cells::InputCells;
use crate::model::CellAddress;
use crate::model::query::{QueryCounters, RangeMembership};

/// Column-ordered key cursor borrowing authoritative edits without scanning other columns.
pub(super) struct InputColumn<'a> {
    pub(super) range: std::collections::btree_set::Range<'a, (u32, u32)>,
    pub(super) edits: &'a InputCells,
    pub(super) counters: QueryCounters,
}

impl<'a> InputColumn<'a> {
    /// Start a column-ordered cursor over the edits inside `area`.
    pub(super) fn new(edits: &'a InputCells, area: RangeMembership) -> Self {
        let first = (area.first.col, area.first.row);
        let last = (area.last.col, area.last.row);
        let range = if first <= last {
            // SAFETY: The branch proves ordered inclusive bounds.
            edits.columns.range(first..=last)
        } else {
            // SAFETY: Equal included/excluded bounds form a valid empty interval.
            edits.columns.range(first..first)
        };
        Self {
            range,
            edits,
            counters: QueryCounters {
                row_seeks: 1,
                column_seeks: 1,
                ..Default::default()
            },
        }
    }

    /// Key lookups performed so far.
    pub(super) fn counters(&self) -> QueryCounters {
        self.counters
    }
}

impl<'a> Iterator for InputColumn<'a> {
    type Item = (&'a CellAddress, &'a CellEdit);

    fn next(&mut self) -> Option<Self::Item> {
        for &(col, row) in self.range.by_ref() {
            self.counters.key_lookups = self.counters.key_lookups.saturating_add(1);
            if let Some(item) = self.edits.edits.get_key_value(&CellAddress { row, col }) {
                self.counters.cells_visited = self.counters.cells_visited.saturating_add(1);
                return Some(item);
            }
        }
        None
    }
}
