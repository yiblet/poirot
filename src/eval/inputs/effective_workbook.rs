//! Read view combining a baseline workbook with scenario inputs.

use super::column_traversal::ColumnTraversal;
use super::effective_stored_cells::EffectiveStoredCells;
use super::input_rows::InputRows;
use super::run_inputs::RunInputs;
use crate::model::query::{QueryCounters, RangeMembership};
use crate::model::{Cell, CellAddress, ModelError, Workbook};

/// Read view combining a baseline workbook with sparse scenario inputs.
/// Evaluation and workbook projection see the same overridden cells while sheet
/// metadata remains owned by the baseline.
#[derive(Clone, Copy)]
pub struct EffectiveWorkbook<'a> {
    pub(super) workbook: &'a Workbook,
    pub(super) inputs: Option<&'a RunInputs>,
}

impl<'a> EffectiveWorkbook<'a> {
    /// Borrow a baseline and optional sparse overrides through one read port.
    pub fn new(workbook: &'a Workbook, inputs: Option<&'a RunInputs>) -> Self {
        Self { workbook, inputs }
    }

    /// Read effective authored content, respecting overrides and deletion markers.
    pub fn cell(self, sheet: usize, address: CellAddress) -> Option<&'a Cell> {
        match self.inputs.and_then(|inputs| inputs.edit(sheet, address)) {
            Some(edit) => edit.cell(),
            None => self.workbook.sheet(sheet)?.source_cell(address),
        }
    }

    /// Read effective content and saved ownership, reusing the baseline storage lookup.
    pub(in crate::eval) fn cell_with_spill(
        self,
        sheet: usize,
        address: CellAddress,
        source_pages: &crate::model::CellPageCache<'a>,
    ) -> Option<(Option<&'a Cell>, bool)> {
        let worksheet = self.workbook.sheet(sheet)?;
        Some(
            match self.inputs.and_then(|inputs| inputs.edit(sheet, address)) {
                Some(edit) => (edit.cell(), worksheet.spill_owner(address).is_some()),
                None => worksheet.source_cell_cached(address, source_pages),
            },
        )
    }

    /// Traverse effective stored cells in row-major order within a rectangle.
    pub fn stored(self, area: RangeMembership) -> Result<EffectiveStoredCells<'a>, ModelError> {
        self.membership(area, false)
    }

    /// Traverse effective stored cells in reverse row-major order.
    pub fn stored_reverse(
        self,
        area: RangeMembership,
    ) -> Result<EffectiveStoredCells<'a>, ModelError> {
        self.membership(area, true)
    }

    /// Merge baseline and edit cursors over one rectangle in either direction.
    fn membership(
        self,
        area: RangeMembership,
        reverse: bool,
    ) -> Result<EffectiveStoredCells<'a>, ModelError> {
        RangeMembership::new(area.sheet, area.first, area.last)?;
        let baseline_sheet = self
            .workbook
            .sheet(area.sheet)
            .ok_or(ModelError::InvalidWorksheetIndex { index: area.sheet })?;
        let baseline = baseline_sheet.cells();
        let overlay = self
            .inputs
            .and_then(|inputs| inputs.sheets.get(&area.sheet));
        // Full-width rectangles are contiguous in CellAddress order. Keep the
        // existing merge cursors open across rows instead of locating each row
        // and reopening its bounded source ranges.
        let contiguous = area.first.col == 0 && area.last.col == crate::model::MAX_COLUMNS - 1;
        let mut base_row = contiguous.then(|| baseline.range(area.first..=area.last));
        let base_next = EffectiveStoredCells::advance_base(&mut base_row, reverse);
        let column = (!reverse && area.first.col == area.last.col)
            .then(|| ColumnTraversal::new(baseline, overlay, area));
        let mut input_rows = overlay
            .filter(|_| column.is_none())
            .map(|cells| InputRows::new(cells, area, reverse));
        let input_next = if contiguous {
            input_rows.as_mut().and_then(InputRows::take)
        } else {
            None
        };
        let mut baseline_rows = (!contiguous && column.is_none())
            .then(|| baseline.occupied_rows(area.first.row, area.last.row));
        let baseline_next_row = baseline_rows.as_mut().and_then(|rows| {
            if reverse {
                rows.next_back()
            } else {
                rows.next()
            }
        });
        let baseline_pages = baseline_rows
            .is_some()
            .then(|| baseline.row_page_range(area.first.col, area.last.col));
        Ok(EffectiveStoredCells {
            baseline_pages,
            baseline_sheet,
            input_rows,
            area,
            reverse,
            needs_row: !contiguous,
            base_row,
            base_next,
            input_next,
            baseline_rows,
            baseline_next_row,
            column,
            counters: QueryCounters {
                // Each contiguous source interval replaces the outer occupied-
                // row lookup and per-row bounded reopen operations below.
                row_seeks: if contiguous {
                    // SAFETY: Boolean conversion is zero or one, so this sum is at most two.
                    1 + u64::from(overlay.is_some())
                } else {
                    0
                },
                ..QueryCounters::default()
            },
        })
    }
}
