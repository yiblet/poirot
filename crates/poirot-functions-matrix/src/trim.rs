//! Border geometry from occupied members, independent of worksheet storage.
use crate::{Result, value};

/// Which empty borders on one axis should be removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimMode {
    /// Keep the original extent.
    None,
    /// Remove empty members before the first occupied coordinate.
    Leading,
    /// Remove empty members after the last occupied coordinate.
    Trailing,
    /// Remove empty members at both ends.
    Both,
}
impl TrimMode {
    /// Decode the documented integer mode without accepting unknown values.
    pub fn new(mode: u8) -> Option<Self> {
        match mode {
            0 => Some(Self::None),
            1 => Some(Self::Leading),
            2 => Some(Self::Trailing),
            3 => Some(Self::Both),
            _ => None,
        }
    }
    fn limits(self, extent: usize, first: usize, last: usize) -> (usize, usize) {
        (
            if matches!(self, Self::Leading | Self::Both) {
                first
            } else {
                0
            },
            if matches!(self, Self::Trailing | Self::Both) {
                last
            } else {
                extent.saturating_sub(1)
            },
        )
    }
}
/// A nonempty retained rectangle, with inclusive zero-based input offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrimRectangle {
    /// First retained row.
    pub first_row: usize,
    /// Last retained row.
    pub last_row: usize,
    /// First retained column.
    pub first_column: usize,
    /// Last retained column.
    pub last_column: usize,
}
/// Constant-space occupied bounds, usable with sparse or dense member traversal.
#[derive(Clone, Copy, Debug)]
pub struct TrimBounds {
    rows: usize,
    columns: usize,
    occupied: Option<TrimRectangle>,
}
impl TrimBounds {
    /// Admit nonempty input geometry without allocating its members.
    pub fn new(rows: usize, columns: usize) -> Result<Self> {
        if rows == 0 || columns == 0 {
            return value();
        }
        Ok(Self {
            rows,
            columns,
            occupied: None,
        })
    }
    /// Include one nonblank coordinate, rejecting coordinates outside the input.
    pub fn include(&mut self, row: usize, column: usize) -> Result<()> {
        if row >= self.rows || column >= self.columns {
            return value();
        }
        self.occupied = Some(match self.occupied {
            None => TrimRectangle {
                first_row: row,
                last_row: row,
                first_column: column,
                last_column: column,
            },
            Some(bounds) => TrimRectangle {
                first_row: bounds.first_row.min(row),
                last_row: bounds.last_row.max(row),
                first_column: bounds.first_column.min(column),
                last_column: bounds.last_column.max(column),
            },
        });
        Ok(())
    }
    /// Return the retained extent, or no rectangle if trimming removes every member.
    pub fn finish(self, rows: TrimMode, columns: TrimMode) -> Option<TrimRectangle> {
        if rows == TrimMode::None && columns == TrimMode::None {
            return Some(TrimRectangle {
                first_row: 0,
                last_row: self.rows.saturating_sub(1),
                first_column: 0,
                last_column: self.columns.saturating_sub(1),
            });
        }
        let occupied = self.occupied?;
        let (first_row, last_row) = rows.limits(self.rows, occupied.first_row, occupied.last_row);
        let (first_column, last_column) =
            columns.limits(self.columns, occupied.first_column, occupied.last_column);
        Some(TrimRectangle {
            first_row,
            last_row,
            first_column,
            last_column,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_axis_modes_retain_interior_holes() {
        for r in 0..4 {
            for c in 0..4 {
                let mut bounds = TrimBounds::new(7, 9).unwrap();
                bounds.include(2, 3).unwrap();
                bounds.include(4, 6).unwrap();
                let actual = bounds
                    .finish(TrimMode::new(r).unwrap(), TrimMode::new(c).unwrap())
                    .unwrap();
                assert_eq!(actual.first_row, if r & 1 != 0 { 2 } else { 0 });
                assert_eq!(actual.last_row, if r & 2 != 0 { 4 } else { 6 });
                assert_eq!(actual.first_column, if c & 1 != 0 { 3 } else { 0 });
                assert_eq!(actual.last_column, if c & 2 != 0 { 6 } else { 8 });
            }
        }
    }
    #[test]
    fn empty_and_invalid_geometry_are_explicit() {
        assert!(TrimBounds::new(0, 1).is_err());
        let mut bounds = TrimBounds::new(2, 3).unwrap();
        assert!(bounds.include(2, 0).is_err());
        assert!(bounds.finish(TrimMode::Both, TrimMode::Both).is_none());
        assert_eq!(
            bounds
                .finish(TrimMode::None, TrimMode::None)
                .unwrap()
                .last_column,
            2
        );
        assert_eq!(TrimMode::new(4), None);
    }
}
