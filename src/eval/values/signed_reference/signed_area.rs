//! Pure logical signed reference geometry and physical read projection.

use super::grid_read::GridRead;
use super::source_dimension::SourceDimension;
use crate::eval::values::area::Area;
use crate::eval::values::area_kind::AreaKind;
use crate::model::CellAddress;

/// Logical reference geometry that can extend beyond the physical worksheet grid.
/// Signed bounds preserve source-compatible dimensions and off-grid blank reads
/// until a consumer projects them onto actual cells.
#[derive(Clone, Copy)]
pub(in crate::eval) struct SignedArea {
    /// Single-cell or area provenance.
    pub(in crate::eval) kind: AreaKind,
    /// Workbook sheet index.
    pub(in crate::eval) sheet: usize,
    /// Inclusive first logical row.
    pub(in crate::eval) r0: i32,
    /// Inclusive last logical row.
    pub(in crate::eval) r1: i32,
    /// Inclusive first logical column.
    pub(in crate::eval) c0: i32,
    /// Inclusive last logical column.
    pub(in crate::eval) c1: i32,
}

impl SignedArea {
    /// Construct ordered signed bounds without clipping to the worksheet grid.
    pub(in crate::eval) fn new(
        kind: AreaKind,
        sheet: usize,
        rows: (i32, i32),
        columns: (i32, i32),
    ) -> Option<Self> {
        (rows.0 <= rows.1 && columns.0 <= columns.1).then_some(Self {
            kind,
            sheet,
            r0: rows.0,
            r1: rows.1,
            c0: columns.0,
            c1: columns.1,
        })
    }

    /// Compute row count with source-compatible wrapping integer arithmetic.
    pub(in crate::eval) fn row_dimension(self) -> SourceDimension {
        SourceDimension::axis(self.r0, self.r1)
    }

    /// Compute column count with source-compatible wrapping integer arithmetic.
    pub(in crate::eval) fn column_dimension(self) -> SourceDimension {
        SourceDimension::axis(self.c0, self.c1)
    }

    /// Count logical row iterations, treating nonpositive dimensions as empty.
    pub(in crate::eval) fn height(self) -> u64 {
        self.row_dimension().iterations()
    }

    /// Count logical column iterations, treating nonpositive dimensions as empty.
    pub(in crate::eval) fn width(self) -> u64 {
        self.column_dimension().iterations()
    }

    /// Return dimensions only when both source dimensions are positive.
    pub(in crate::eval) fn positive_shape(self) -> Option<(usize, usize)> {
        match (self.row_dimension(), self.column_dimension()) {
            (SourceDimension::Positive(r), SourceDimension::Positive(c)) => {
                Some((r as usize, c as usize))
            }
            _ => None,
        }
    }

    /// Empty physical intersection is distinct from the nonempty logical shape.
    pub(in crate::eval) fn physical(self) -> Option<Area> {
        if self.height() == 0 || self.width() == 0 {
            return None;
        }
        let r0 = self.r0.max(0);
        let c0 = self.c0.max(0);
        let r1 = (if self.r0 <= self.r1 {
            self.r1
        } else {
            i32::MAX
        })
        .min(1_048_575);
        let c1 = (if self.c0 <= self.c1 {
            self.c1
        } else {
            i32::MAX
        })
        .min(16_383);
        (r0 <= r1 && c0 <= c1).then_some(Area {
            kind: self.kind,
            sheet: self.sheet,
            r0: r0 as u32,
            r1: r1 as u32,
            c0: c0 as u32,
            c1: c1 as u32,
        })
    }

    /// Preserve reference-origin semantics for an off-grid blank, without a workbook read.
    pub(in crate::eval) fn read(self, row: u64, column: u64) -> Option<GridRead> {
        if row >= self.height() || column >= self.width() {
            return None;
        }
        let row = self.r0.wrapping_add(row as i32);
        let column = self.c0.wrapping_add(column as i32);
        Some(
            if !(0..1_048_576).contains(&row) || !(0..16_384).contains(&column) {
                GridRead::Blank
            } else {
                GridRead::Cell {
                    sheet: self.sheet,
                    address: CellAddress {
                        row: row as u32,
                        col: column as u32,
                    },
                }
            },
        )
    }

    /// Count logical member cells, including off-grid blanks.
    pub(in crate::eval) fn size(self) -> u64 {
        self.height() * self.width()
    }

    /// Promote physical bounds to equivalent signed logical bounds.
    pub(in crate::eval) fn from_unsigned(a: Area) -> Self {
        Self {
            kind: a.kind,
            sheet: a.sheet,
            r0: a.r0 as i32,
            r1: a.r1 as i32,
            c0: a.c0 as i32,
            c1: a.c1 as i32,
        }
    }

    /// Select a scalar using caller-coordinate implicit intersection rules.
    pub(in crate::eval) fn implicit(self, at: CellAddress) -> Option<GridRead> {
        let (row, col) = (at.row as i32, at.col as i32);
        let contains_row = (self.r0..=self.r1).contains(&row);
        let contains_col = (self.c0..=self.c1).contains(&col);
        let (row, col) = if self.size() == 1 {
            (self.r0, self.c0)
        } else if self.width() == 1 && contains_row {
            (row, self.c0)
        } else if self.height() == 1 && contains_col {
            (self.r0, col)
        } else if contains_row && contains_col {
            (row, col)
        } else {
            return None;
        };
        Some(self.absolute(row, col))
    }

    fn absolute(self, row: i32, col: i32) -> GridRead {
        if !(0..1_048_576).contains(&row) || !(0..16_384).contains(&col) {
            GridRead::Blank
        } else {
            GridRead::Cell {
                sheet: self.sheet,
                address: CellAddress {
                    row: row as u32,
                    col: col as u32,
                },
            }
        }
    }

    /// Project zero-based offsets using source-compatible wrapping coordinates.
    pub(in crate::eval) fn relative(self, row: usize, col: usize) -> Option<GridRead> {
        Some(self.absolute(
            self.r0.wrapping_add(i32::try_from(row).ok()?),
            self.c0.wrapping_add(i32::try_from(col).ok()?),
        ))
    }

    /// Apply one-based INDEX selection, with zero retaining the whole axis.
    pub(in crate::eval) fn select(self, row: usize, col: usize) -> Self {
        let r = if row == 0 {
            None
        } else {
            Some(self.r0.wrapping_add(row as i32).wrapping_sub(1))
        };
        let c = if col == 0 {
            None
        } else {
            Some(self.c0.wrapping_add(col as i32).wrapping_sub(1))
        };
        Self {
            kind: AreaKind::Area,
            r0: r.unwrap_or(self.r0),
            r1: r.unwrap_or(self.r1),
            c0: c.unwrap_or(self.c0),
            c1: c.unwrap_or(self.c1),
            ..self
        }
    }

    /// Rebuild source-ordered bounds relative to this area's origin.
    pub(in crate::eval) fn source_bounds(self, rows: (i32, i32), cols: (i32, i32)) -> Self {
        let (a, b) = (rows.0.wrapping_sub(self.r0), rows.1.wrapping_sub(self.r0));
        let (c, d) = (cols.0.wrapping_sub(self.c0), cols.1.wrapping_sub(self.c0));
        Self {
            kind: AreaKind::Area,
            r0: self.r0.wrapping_add(a.min(b)),
            r1: self.r0.wrapping_add(a.max(b)),
            c0: self.c0.wrapping_add(c.min(d)),
            c1: self.c0.wrapping_add(c.max(d)),
            ..self
        }
    }
}
