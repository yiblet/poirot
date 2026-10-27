//! Pure worksheet read selection; plans never read cells or consume budgets.
use crate::eval::values::{Area, VALUE_ERROR};
use crate::model::CellError::Ref;
use crate::model::{CellAddress, CellError, MAX_COLUMNS, MAX_ROWS};

/// A selected worksheet read; executing it remains the host's responsibility.
pub(super) struct CellReadPlan {
    /// Selected workbook sheet.
    pub(super) sheet: usize,
    /// Selected physical coordinates.
    pub(super) address: CellAddress,
}

impl CellReadPlan {
    /// Select one physical cell by caller-coordinate implicit intersection.
    pub(super) fn implicit(area: Area, at: CellAddress) -> Result<Self, CellError> {
        let address = if area.size() == 1 {
            CellAddress {
                row: area.r0,
                col: area.c0,
            }
        } else if area.r0 == area.r1 && (area.c0..=area.c1).contains(&at.col) {
            CellAddress {
                row: area.r0,
                col: at.col,
            }
        } else if area.c0 == area.c1 && (area.r0..=area.r1).contains(&at.row) {
            CellAddress {
                row: at.row,
                col: area.c0,
            }
        } else if area.contains(at) {
            at
        } else {
            return Err(VALUE_ERROR);
        };
        Ok(Self {
            sheet: area.sheet,
            address,
        })
    }

    /// Project offsets from an area's origin, allowing resizing within worksheet bounds.
    pub(super) fn relative(area: Area, row: usize, column: usize) -> Result<Self, CellError> {
        let absolute_row = u64::from(area.r0)
            .checked_add(row as u64)
            .filter(|row| *row < u64::from(MAX_ROWS))
            .ok_or(Ref)?;
        let absolute_column = u64::from(area.c0)
            .checked_add(column as u64)
            .filter(|column| *column < u64::from(MAX_COLUMNS))
            .ok_or(Ref)?;
        Ok(Self {
            sheet: area.sheet,
            address: CellAddress {
                row: absolute_row as u32,
                col: absolute_column as u32,
            },
        })
    }
}
