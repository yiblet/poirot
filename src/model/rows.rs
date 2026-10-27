//! Sparse row lifecycle, independent of parser, evaluator and file adapters.
use crate::model::{Cell, CellAddress, MAX_COLUMNS, MAX_ROWS, Worksheet};
use poirot_io::{ModelError, RowVisibility};

fn validate(row: u32) -> Result<(), ModelError> {
    if row >= MAX_ROWS {
        Err(ModelError::InvalidRow { row })
    } else {
        Ok(())
    }
}
impl Worksheet {
    /// Read visibility flags; absent metadata uses visible defaults.
    pub fn row_visibility(&self, index: u32) -> RowVisibility {
        self.visibility.get(&index).copied().unwrap_or_default()
    }
    /// Nonvisible rows acquire sparse row presence; restoring visibility retains presence.
    pub fn set_row_visibility(
        &mut self,
        index: u32,
        visibility: RowVisibility,
    ) -> Result<(), ModelError> {
        validate(index)?;
        if visibility == RowVisibility::default() {
            self.visibility.remove(&index);
        } else {
            self.rows.insert(index);
            self.visibility.insert(index, visibility);
        }
        Ok(())
    }
    /// Iterate explicitly stored row-visibility overrides.
    pub fn row_visibility_entries(
        &self,
    ) -> impl ExactSizeIterator<Item = (u32, RowVisibility)> + '_ {
        self.visibility.iter().map(|(row, flags)| (*row, *flags))
    }

    /// Create presence without allocating missing cells. Existing rows retain cells.
    pub fn create_row(&mut self, index: u32) -> Result<RowMut<'_>, ModelError> {
        validate(index)?;
        self.rows.insert(index);
        Ok(RowMut { sheet: self, index })
    }
    /// Borrow a logical row without creating source cells.
    pub fn row(&self, index: u32) -> Option<Row<'_>> {
        self.rows
            .contains(&index)
            .then_some(Row { sheet: self, index })
    }
    /// Borrow a row mutation facade with worksheet bounds validation.
    pub fn row_mut(&mut self, index: u32) -> Option<RowMut<'_>> {
        if self.rows.contains(&index) {
            Some(RowMut { sheet: self, index })
        } else {
            None
        }
    }
    /// Sorted indices of present rows, including empty rows; no dense allocation.
    pub fn row_indices(&self) -> impl DoubleEndedIterator<Item = u32> + ExactSizeIterator + '_ {
        self.rows.iter().copied()
    }
    /// Remove presence and every cell in the row. Other coordinates never shift.
    pub fn remove_row(&mut self, index: u32) -> Result<bool, ModelError> {
        validate(index)?;
        self.visibility.remove(&index);
        if !self.rows.remove(&index) {
            return Ok(false);
        }
        let addresses: Vec<_> = self
            .cells()
            .range(
                CellAddress { row: index, col: 0 }..=CellAddress {
                    row: index,
                    col: MAX_COLUMNS - 1,
                },
            )
            .map(|(a, _)| *a)
            .collect();
        for address in addresses {
            self.remove(address)?;
        }
        Ok(true)
    }
}
/// Borrowed view of persisted row presence. Removing the last cell retains its row.
#[derive(Clone, Copy)]
pub struct Row<'a> {
    sheet: &'a Worksheet,
    index: u32,
}
impl<'a> Row<'a> {
    /// Return the zero-based worksheet row index.
    pub fn index(self) -> u32 {
        self.index
    }
    /// Borrow a stored cell at the selected column.
    pub fn get(self, column: u32) -> Option<&'a Cell> {
        self.sheet.get_validated(CellAddress {
            row: self.index,
            col: column,
        })
    }
    /// Iterate this row's explicitly stored cells.
    pub fn cells(self) -> impl DoubleEndedIterator<Item = (u32, &'a Cell)> {
        self.sheet
            .cells()
            .range(
                CellAddress {
                    row: self.index,
                    col: 0,
                }..=CellAddress {
                    row: self.index,
                    col: MAX_COLUMNS - 1,
                },
            )
            .map(|(a, c)| (a.col, c))
    }
}
/// Exclusive view of an explicitly present worksheet row. Column indices are zero-based;
/// setting cells retains row presence, and removing a cell does not shift other columns.
pub struct RowMut<'a> {
    sheet: &'a mut Worksheet,
    index: u32,
}
impl RowMut<'_> {
    /// Return the zero-based worksheet row index.
    pub fn index(&self) -> u32 {
        self.index
    }
    /// Borrow an existing cell in this row.
    pub fn get(&self, column: u32) -> Option<&Cell> {
        self.sheet.get_validated(CellAddress {
            row: self.index,
            col: column,
        })
    }
    /// Mutably borrow an existing cell in this row.
    pub fn get_mut(&mut self, column: u32) -> Option<&mut Cell> {
        self.sheet.get_mut_validated(CellAddress {
            row: self.index,
            col: column,
        })
    }
    /// Validate the column and replace the row's authored cell.
    pub fn set(&mut self, column: u32, cell: Cell) -> Result<Option<Cell>, ModelError> {
        self.sheet.set(CellAddress::new(self.index, column)?, cell)
    }
    /// Removal retains row presence, including when its last cell is removed.
    pub fn remove(&mut self, column: u32) -> Result<Option<Cell>, ModelError> {
        self.sheet.remove(CellAddress::new(self.index, column)?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visibility_lifecycle_is_sparse_bounded_and_cloned() {
        let mut workbook = crate::model::Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        let flags = RowVisibility {
            manually_hidden: true,
            filtered_out: true,
        };
        sheet.set_row_visibility(99, flags).unwrap();
        assert_eq!(sheet.row_indices().collect::<Vec<_>>(), vec![99]);
        assert_eq!(sheet.row_visibility(98), RowVisibility::default());
        assert!(sheet.set_row_visibility(MAX_ROWS, flags).is_err());
        assert_eq!(
            sheet.row_visibility_entries().collect::<Vec<_>>(),
            vec![(99, flags)]
        );
        let copy = workbook.clone();
        assert_eq!(copy.sheet(0).unwrap().row_visibility(99), flags);
        assert_eq!(
            crate::model::clone_footprint::CloneFootprint::measure(&copy)
                .unwrap()
                .row_visibility,
            1
        );
        let sheet = workbook.sheet_mut(0).unwrap();
        sheet
            .set_row_visibility(99, RowVisibility::default())
            .unwrap();
        assert!(sheet.row(99).is_some());
        assert_eq!(sheet.row_visibility_entries().len(), 0);
        sheet.set_row_visibility(99, flags).unwrap();
        sheet.remove_row(99).unwrap();
        assert_eq!(sheet.row_visibility(99), RowVisibility::default());
        assert!(sheet.row(99).is_none());
    }
    #[test]
    fn sparse_lifecycle_and_atomic_bounds() {
        let mut sheet = Worksheet::new("Rows").unwrap();
        assert!(sheet.row(9).is_none());
        sheet.create_row(9).unwrap();
        assert!(sheet.row(9).unwrap().cells().next().is_none());
        assert!(sheet.create_row(MAX_ROWS).is_err());
        {
            let mut row = sheet.row_mut(9).unwrap();
            row.set(2, Cell::new(4.0)).unwrap();
            assert!(row.set(MAX_COLUMNS, Cell::new(5.0)).is_err());
            assert_eq!(
                row.remove(2).unwrap().unwrap().value,
                crate::model::CellValue::Number(4.0)
            );
        }
        assert!(sheet.row(9).is_some());
        sheet
            .set_value(CellAddress::new(MAX_ROWS - 1, 1).unwrap(), 1.0)
            .unwrap();
        assert_eq!(
            sheet.row_indices().collect::<Vec<_>>(),
            vec![9, MAX_ROWS - 1]
        );
        assert!(sheet.remove_row(MAX_ROWS).is_err());
        assert!(sheet.remove_row(MAX_ROWS - 1).unwrap());
        assert!(sheet.cells().is_empty());
        assert!(sheet.remove_row(9).unwrap());
        assert!(!sheet.remove_row(9).unwrap());
    }
}
