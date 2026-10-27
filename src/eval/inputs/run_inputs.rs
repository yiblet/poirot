//! Sparse scenario assignments bound to one workbook source.

use super::cell_edit::CellEdit;
use super::input_cells::InputCells;
use super::input_error::InputError;
use crate::model::{Cell, CellAddress, CellValue, ModelError, Workbook, WorkbookChange};
use std::collections::BTreeMap;

/// Sparse scenario assignments prepared for one shared workbook source.
/// The source token prevents applying a batch to another baseline, while unique
/// resolved coordinates let evaluation merge edits without ambiguous duplicates.
pub struct RunInputs {
    /// Exclusive source edits in sparse coordinate order.
    pub(in crate::eval) sheets: BTreeMap<usize, InputCells>,
    /// Source identity token binding assignments to their shared workbook owner.
    pub(in crate::eval) source: std::sync::Arc<()>,
}

impl RunInputs {
    /// Create empty sparse inputs with a fresh source token.
    pub(in crate::eval) fn empty() -> Self {
        Self {
            sheets: BTreeMap::new(),
            source: std::sync::Arc::new(()),
        }
    }

    /// Validate unique literal assignments and preserve baseline cell styles.
    pub(in crate::eval) fn new(
        workbook: &Workbook,
        assignments: impl IntoIterator<Item = (usize, CellAddress, CellValue)>,
        max_assignments: usize,
        source: std::sync::Arc<()>,
    ) -> Result<Self, InputError> {
        let mut inputs = Self {
            sheets: BTreeMap::new(),
            source,
        };
        let mut count = 0usize;
        for (sheet, address, value) in assignments {
            address.validate().map_err(ModelError::from)?;
            let worksheet = workbook
                .sheet(sheet)
                .ok_or(ModelError::InvalidWorksheetIndex { index: sheet })?;
            count = count.checked_add(1).ok_or(InputError::CountOverflow)?;
            if count > max_assignments {
                return Err(InputError::Limit {
                    limit: max_assignments,
                    requested: count,
                });
            }
            let mut cell = Cell::new(value);
            if let Some(original) = worksheet.get_validated(address) {
                cell.style = original.style.clone();
            }
            if inputs
                .sheets
                .entry(sheet)
                .or_default()
                .set_cell(address, cell, false)?
            {
                return Err(InputError::Duplicate { sheet, address });
            }
        }
        Ok(inputs)
    }

    /// Read an explicit override without falling back to the baseline.
    pub fn cell(&self, sheet: usize, address: CellAddress) -> Option<&Cell> {
        self.edit(sheet, address)?.cell()
    }

    /// Read an authored edit without consulting baseline content.
    pub(in crate::eval) fn edit(&self, sheet: usize, address: CellAddress) -> Option<&CellEdit> {
        self.sheets.get(&sheet)?.get(&address)
    }

    /// Whether a coordinate has an explicit deletion marker.
    pub(in crate::eval) fn is_deleted(&self, sheet: usize, address: CellAddress) -> bool {
        matches!(self.edit(sheet, address), Some(CellEdit::Delete))
    }

    /// Traverse all edits once in sheet and coordinate order.
    pub(in crate::eval) fn edits(&self) -> impl Iterator<Item = (usize, CellAddress, &CellEdit)> {
        self.sheets.iter().flat_map(|(&sheet, cells)| {
            cells
                .cells()
                .iter()
                .map(move |(&address, edit)| (sheet, address, edit))
        })
    }

    /// Whether no cell has an authored edit.
    pub(in crate::eval) fn is_empty(&self) -> bool {
        self.edits().next().is_none()
    }

    /// Express authored edits as workbook changes: removals first, then writes and
    /// style-only edits, each in sheet and coordinate order.
    pub(in crate::eval) fn workbook_changes(&self) -> impl Iterator<Item = WorkbookChange> + '_ {
        let deleted = self.edits().filter_map(|(sheet, address, edit)| {
            matches!(edit, CellEdit::Delete)
                .then_some(WorkbookChange::RemoveCell { sheet, address })
        });
        let writes = self
            .edits()
            .filter_map(|(sheet, address, edit)| match edit {
                CellEdit::Delete => None,
                CellEdit::StyleOnly(cell) => Some(WorkbookChange::SetStyle {
                    sheet,
                    address,
                    style: cell.style.clone(),
                }),
                CellEdit::Replace(cell) => Some(WorkbookChange::SetCell {
                    sheet,
                    address,
                    cell: cell.as_ref().clone(),
                }),
            });
        deleted.chain(writes)
    }
}
