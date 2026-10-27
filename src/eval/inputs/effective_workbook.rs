//! Read view combining a baseline workbook with scenario inputs.

use super::column_traversal::ColumnTraversal;
use super::effective_stored_cells::EffectiveStoredCells;
use super::overlay_cells::OverlayCells;
use super::overlay_range::advance;
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
            .ok_or_else(|| ModelError("query worksheet index out of bounds".into()))?;
        let baseline = baseline_sheet.cells();
        let overlay = self
            .inputs
            .and_then(|inputs| inputs.sheets.get(&area.sheet));
        // Full-width rectangles are contiguous in CellAddress order. Keep the
        // existing merge cursors open across rows instead of locating each row
        // and reopening its bounded source ranges.
        let contiguous = area.first.col == 0 && area.last.col == crate::model::MAX_COLUMNS - 1;
        let mut base_row =
            contiguous.then(|| OverlayCells::Base(baseline).range(area.first..=area.last));
        let mut input_row = overlay
            .filter(|_| contiguous)
            .map(|cells| OverlayCells::Input(cells).range(area.first..=area.last));
        let base_next = advance(&mut base_row, reverse);
        let input_next = advance(&mut input_row, reverse);
        Ok(EffectiveStoredCells {
            baseline,
            baseline_sheet,
            overlay,
            area,
            reverse,
            next_row: (!contiguous).then_some(if reverse {
                area.last.row
            } else {
                area.first.row
            }),
            base_row,
            input_row,
            base_next,
            input_next,
            column: (!reverse && area.first.col == area.last.col)
                .then(|| ColumnTraversal::new(baseline, overlay, area)),
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
