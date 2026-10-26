//! Pure logical signed reference geometry and physical read projection.
use super::{Area, AreaKind};
use crate::model::CellAddress;

/// Logical reference geometry that can extend beyond the physical worksheet grid.
/// Signed bounds preserve source-compatible dimensions and off-grid blank reads
/// until a consumer projects them onto actual cells.
#[derive(Clone, Copy)]
pub(super) struct SignedArea {
    /// Single-cell or area provenance.
    pub(super) kind: AreaKind,
    /// Workbook sheet index.
    pub(super) sheet: usize,
    /// Inclusive first logical row.
    pub(super) r0: i32,
    /// Inclusive last logical row.
    pub(super) r1: i32,
    /// Inclusive first logical column.
    pub(super) c0: i32,
    /// Inclusive last logical column.
    pub(super) c1: i32,
}
/// Physical projection of a logical reference read.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum GridRead {
    /// Off-grid logical cell, yielding a reference-origin blank.
    Blank,
    /// In-grid workbook cell to read normally.
    Cell {
        /// Workbook sheet index.
        sheet: usize,
        /// Physical cell coordinates.
        address: CellAddress,
    },
}
/// AreaEvalBase computes dimensions with Java int arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceDimension {
    /// Positive dimension usable for iteration.
    Positive(u32),
    /// Java arithmetic produced zero.
    Zero,
    /// Java signed arithmetic overflowed to a negative value.
    NegativeOverflow(i32),
}
impl SourceDimension {
    fn axis(first: i32, last: i32) -> Self {
        match last.wrapping_sub(first).wrapping_add(1) {
            0 => Self::Zero,
            n if n > 0 => Self::Positive(n as u32),
            n => Self::NegativeOverflow(n),
        }
    }
    /// Return the source-compatible signed dimension.
    pub(super) fn value(self) -> i32 {
        match self {
            Self::Positive(n) => n as i32,
            Self::Zero => 0,
            Self::NegativeOverflow(n) => n,
        }
    }
    fn iterations(self) -> u64 {
        self.value().max(0) as u64
    }
}
impl SignedArea {
    /// Construct ordered signed bounds without clipping to the worksheet grid.
    pub(super) fn new(
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
    pub(super) fn row_dimension(self) -> SourceDimension {
        SourceDimension::axis(self.r0, self.r1)
    }
    /// Compute column count with source-compatible wrapping integer arithmetic.
    pub(super) fn column_dimension(self) -> SourceDimension {
        SourceDimension::axis(self.c0, self.c1)
    }
    /// Count logical row iterations, treating nonpositive dimensions as empty.
    pub(super) fn height(self) -> u64 {
        self.row_dimension().iterations()
    }
    /// Count logical column iterations, treating nonpositive dimensions as empty.
    pub(super) fn width(self) -> u64 {
        self.column_dimension().iterations()
    }
    /// Return dimensions only when both source dimensions are positive.
    pub(super) fn positive_shape(self) -> Option<(usize, usize)> {
        match (self.row_dimension(), self.column_dimension()) {
            (SourceDimension::Positive(r), SourceDimension::Positive(c)) => {
                Some((r as usize, c as usize))
            }
            _ => None,
        }
    }
    /// Empty physical intersection is distinct from the nonempty logical shape.
    pub(super) fn physical(self) -> Option<Area> {
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
    pub(super) fn read(self, row: u64, column: u64) -> Option<GridRead> {
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
}
impl SignedArea {
    /// Count logical member cells, including off-grid blanks.
    pub(super) fn size(self) -> u64 {
        self.height() * self.width()
    }
    /// Promote physical bounds to equivalent signed logical bounds.
    pub(super) fn from_unsigned(a: Area) -> Self {
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
    pub(super) fn implicit(self, at: CellAddress) -> Option<GridRead> {
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
    pub(super) fn relative(self, row: usize, col: usize) -> Option<GridRead> {
        Some(self.absolute(
            self.r0.wrapping_add(i32::try_from(row).ok()?),
            self.c0.wrapping_add(i32::try_from(col).ok()?),
        ))
    }

    /// Apply one-based INDEX selection, with zero retaining the whole axis.
    pub(super) fn select(self, row: usize, col: usize) -> Self {
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
    fn source_bounds(self, rows: (i32, i32), cols: (i32, i32)) -> Self {
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

use super::{BinaryOp, EvalResult, Null, Ref, Session, VALUE_ERROR, Value, WorkKind, err};
impl Session<'_> {
    /// Read an in-grid cell or produce a logical off-grid blank.
    pub(super) fn signed_read(&mut self, plan: GridRead) -> EvalResult<super::CellValue> {
        match plan {
            GridRead::Blank => Ok(super::Blank),
            GridRead::Cell { sheet, address } => self.cell(sheet, address),
        }
    }
    /// Read a signed range member, borrowing eligible authored literals directly.
    pub(super) fn signed_range_read(&mut self, plan: GridRead) -> EvalResult<super::CellValue> {
        match plan {
            GridRead::Blank => Ok(super::Blank),
            GridRead::Cell { sheet, address } => self.range_cell(sheet, address),
        }
    }
    fn signed_areas(&mut self, value: Value) -> EvalResult<Option<Vec<SignedArea>>> {
        Ok(match value {
            Value::SignedRefs(areas) => Some(areas),
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                self.budget.allocation(areas.len())?;
                Some(areas.into_iter().map(SignedArea::from_unsigned).collect())
            }
            _ => None,
        })
    }
    /// Execute union, range, or intersection while retaining signed geometry.
    pub(super) fn signed_references(
        &mut self,
        op: BinaryOp,
        left: Value,
        right: Value,
    ) -> EvalResult<Value> {
        if let Value::Scalar(super::Error(error)) | Value::SelectedScalar(super::Error(error), _) =
            &left
        {
            return Ok(err(*error));
        }
        if let Value::Scalar(super::Error(error)) | Value::SelectedScalar(super::Error(error), _) =
            &right
        {
            return Ok(err(*error));
        }
        let (Some(left), Some(right)) = (self.signed_areas(left)?, self.signed_areas(right)?)
        else {
            return Ok(err(VALUE_ERROR));
        };
        match op {
            BinaryOp::Union => {
                self.budget
                    .allocation(left.len().saturating_add(right.len()))?;
                Ok(Value::SignedRefs(left.into_iter().chain(right).collect()))
            }
            BinaryOp::Range => {
                let (Some(mut envelope), Some(right_first)) =
                    (left.first().copied(), right.first().copied())
                else {
                    return Ok(err(Ref));
                };
                if right_first.sheet != envelope.sheet {
                    return Ok(err(Ref));
                }
                for area in left.iter().skip(1).chain(right.iter().skip(1)) {
                    self.budget.charge(WorkKind::ReferencePair, 1)?;
                    if area.sheet != envelope.sheet {
                        return Ok(err(Ref));
                    }
                    envelope.r0 = envelope.r0.min(area.r0);
                    envelope.r1 = envelope.r1.max(area.r1);
                    envelope.c0 = envelope.c0.min(area.c0);
                    envelope.c1 = envelope.c1.max(area.c1);
                }
                self.budget.allocation(1)?;
                Ok(Value::SignedRefs(vec![envelope.source_bounds(
                    (
                        envelope.r0.min(right_first.r0),
                        envelope.r1.max(right_first.r1),
                    ),
                    (
                        envelope.c0.min(right_first.c0),
                        envelope.c1.max(right_first.c1),
                    ),
                )]))
            }
            BinaryOp::Intersection => {
                let mut output = Vec::new();
                for a in left {
                    for b in &right {
                        self.budget.charge(WorkKind::ReferencePair, 1)?;
                        if a.sheet == b.sheet
                            && let Some(area) = SignedArea::new(
                                AreaKind::Area,
                                a.sheet,
                                (a.r0.max(b.r0), a.r1.min(b.r1)),
                                (a.c0.max(b.c0), a.c1.min(b.c1)),
                            )
                        {
                            self.budget.check_allocation(output.len() + 1)?;
                            self.budget.charge(WorkKind::Materialization, 1)?;
                            output.push(a.source_bounds((area.r0, area.r1), (area.c0, area.c1)));
                        }
                    }
                }
                Ok(if output.is_empty() {
                    err(Null)
                } else {
                    Value::SignedRefs(output)
                })
            }
            _ => Ok(err(Ref)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_shape_and_sparse_grid_projection_remain_distinct() {
        let area = SignedArea::new(AreaKind::Area, 2, (-1, 2), (-2, 1)).unwrap();
        assert_eq!((area.height(), area.width()), (4, 4));
        let physical = area.physical().unwrap();
        assert_eq!(
            (
                physical.sheet,
                physical.r0,
                physical.r1,
                physical.c0,
                physical.c1
            ),
            (2, 0, 2, 0, 1)
        );
        assert_eq!(area.read(0, 3), Some(GridRead::Blank));
        assert_eq!(area.read(1, 0), Some(GridRead::Blank));
        assert_eq!(
            area.read(1, 2),
            Some(GridRead::Cell {
                sheet: 2,
                address: CellAddress { row: 0, col: 0 }
            })
        );
        assert_eq!(
            area.read(3, 3),
            Some(GridRead::Cell {
                sheet: 2,
                address: CellAddress { row: 2, col: 1 }
            })
        );
        assert_eq!(area.read(4, 0), None);
    }
    #[test]
    fn wholly_off_grid_area_keeps_nonempty_logical_shape() {
        let area = SignedArea::new(AreaKind::Area, 0, (-2, -1), (-3, -1)).unwrap();
        assert_eq!((area.height(), area.width()), (2, 3));
        assert!(area.physical().is_none());
        assert_eq!(area.read(1, 2), Some(GridRead::Blank));
        assert!(SignedArea::new(AreaKind::Area, 0, (2, 1), (0, 0)).is_none());
    }
}
