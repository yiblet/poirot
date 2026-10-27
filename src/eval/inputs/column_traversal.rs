//! Merged column traversal of baseline and override cursors.

use super::input_cells::InputCells;
use super::input_column::InputColumn;
use super::stored_source::StoredSource;
use crate::model::CellAddress;
use crate::model::query::{QueryCounters, RangeMembership};

/// Merge baseline and override column cursors without a separate accounting walk.
pub(super) struct ColumnTraversal<'a> {
    pub(super) base: crate::model::ColumnCells<'a>,
    pub(super) input: Option<InputColumn<'a>>,
    pub(super) base_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    pub(super) input_next: Option<(&'a CellAddress, StoredSource<'a>)>,
    pub(super) started: bool,
    pub(super) finished: bool,
}

impl<'a> ColumnTraversal<'a> {
    /// Start a column-ordered merge over one rectangle.
    pub(super) fn new(
        base: crate::model::WorksheetCells<'a>,
        input: Option<&'a InputCells>,
        area: RangeMembership,
    ) -> Self {
        Self {
            base: base.column(area.first.col, area.first.row, area.last.row),
            input: input.map(|cells| InputColumn::new(cells, area)),
            base_next: None,
            input_next: None,
            started: false,
            finished: false,
        }
    }

    /// Advance the merge, charging source seeks and lookups to `counters`.
    pub(super) fn next(
        &mut self,
        counters: &mut QueryCounters,
    ) -> Option<(&'a CellAddress, StoredSource<'a>)> {
        if self.finished {
            return None;
        }
        if !self.started {
            self.started = true;
            self.base_next = self
                .base
                .next()
                .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
            self.input_next = self
                .input
                .as_mut()
                .and_then(Iterator::next)
                .map(|(at, edit)| (at, StoredSource::Edit(edit)));
        }
        let selected = match (self.base_next, self.input_next) {
            (Some(base), Some(input)) if base.0 == input.0 => {
                self.base_next = self
                    .base
                    .next()
                    .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
                self.input_next = self
                    .input
                    .as_mut()
                    .and_then(Iterator::next)
                    .map(|(at, edit)| (at, StoredSource::Edit(edit)));
                Some(input)
            }
            (Some(base), Some(input)) if base.0 < input.0 => {
                self.base_next = self
                    .base
                    .next()
                    .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
                Some(base)
            }
            (_, Some(input)) => {
                self.input_next = self
                    .input
                    .as_mut()
                    .and_then(Iterator::next)
                    .map(|(at, edit)| (at, StoredSource::Edit(edit)));
                Some(input)
            }
            (Some(base), None) => {
                self.base_next = self
                    .base
                    .next()
                    .map(|(at, cell)| (at, StoredSource::Baseline(cell)));
                Some(base)
            }
            (None, None) => None,
        };
        *counters = self.base.counters();
        if let Some(input) = &self.input {
            let input = input.counters();
            counters.row_seeks = counters.row_seeks.saturating_add(input.row_seeks);
            counters.column_seeks = counters.column_seeks.saturating_add(input.column_seeks);
            counters.cells_visited = counters.cells_visited.saturating_add(input.cells_visited);
            counters.key_lookups = counters.key_lookups.saturating_add(input.key_lookups);
        }
        self.finished = selected.is_none();
        selected
    }
}
