//! Pure operand selection; these policies never read cells or consume budgets.
use super::{Area, CellAddress, CellError, MAX_COLUMNS, MAX_ROWS, Ref, VALUE_ERROR};

/// This host port models OperandResolver.getSingleValue. Array-mode
/// NumericFunction evaluation is a separate capability, not an implicit override.
#[derive(Clone, Copy)]
pub(super) enum ScalarSelectionPolicy {
    /// Source-compatible caller-cell selection for references and anchored arrays.
    PoiOperandResolver,
}
impl ScalarSelectionPolicy {
    /// Select array offsets without reading values, preserving unknown-anchor failures.
    pub(super) fn array_position(
        self,
        view: &crate::eval::ArrayView,
        at: CellAddress,
    ) -> Result<Result<(usize, usize), CellError>, &'static str> {
        let height = view.len();
        let width = view.first().map_or(0, Vec::len);
        if height == 1 && width == 1 {
            return Ok(Ok((0, 0)));
        }
        let anchor = view
            .anchor
            .ok_or("computed array scalar coordinates are not established by its adapter")?;
        let row = at.row.checked_sub(anchor.row).map(|n| n as usize);
        let col = at.col.checked_sub(anchor.col).map(|n| n as usize);
        let selected = if width == 1 {
            row.map(|r| (r, 0))
        } else if height == 1 {
            col.map(|c| (0, c))
        } else {
            row.zip(col)
        };
        Ok(selected
            .filter(|(r, c)| *r < height && *c < width)
            .ok_or(VALUE_ERROR))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{AreaKind, ArrayView};
    use crate::model::CellValue::Blank;

    #[test]
    fn scalar_arrays_intersect_rows_columns_and_rectangles() {
        let anchor = CellAddress { row: 3, col: 4 };
        let policy = ScalarSelectionPolicy::PoiOperandResolver;
        let row = ArrayView::anchored(vec![vec![Blank; 2]], Some(anchor));
        assert_eq!(
            policy.array_position(&row, CellAddress { row: 90, col: 5 }),
            Ok(Ok((0, 1)))
        );
        let column = ArrayView::anchored(vec![vec![Blank]; 2], Some(anchor));
        assert_eq!(
            policy.array_position(&column, CellAddress { row: 4, col: 90 }),
            Ok(Ok((1, 0)))
        );
        let rectangle = ArrayView::anchored(vec![vec![Blank; 2]; 2], Some(anchor));
        assert_eq!(
            policy.array_position(&rectangle, CellAddress { row: 4, col: 5 }),
            Ok(Ok((1, 1)))
        );
        assert_eq!(
            policy.array_position(&rectangle, CellAddress { row: 2, col: 5 }),
            Ok(Err(VALUE_ERROR))
        );
        assert!(
            policy
                .array_position(&ArrayView::unknown(vec![vec![Blank; 2]]), anchor)
                .is_err()
        );
        assert_eq!(
            policy.array_position(&ArrayView::unknown(vec![vec![Blank]]), anchor),
            Ok(Ok((0, 0)))
        );
    }

    #[test]
    fn reference_intersection_and_relative_resize_have_separate_bounds() {
        let area = Area {
            kind: AreaKind::Area,
            sheet: 2,
            r0: 3,
            r1: 3,
            c0: 4,
            c1: 5,
        };
        let selected = CellReadPlan::implicit(area, CellAddress { row: 90, col: 5 }).unwrap();
        assert_eq!(selected.sheet, 2);
        assert_eq!(selected.address, CellAddress { row: 3, col: 5 });
        assert!(matches!(
            CellReadPlan::implicit(area, CellAddress { row: 3, col: 6 }),
            Err(VALUE_ERROR)
        ));
        let resized = CellReadPlan::relative(area, 2, 3).unwrap();
        assert_eq!(resized.address, CellAddress { row: 5, col: 7 });
        assert!(matches!(
            CellReadPlan::relative(area, MAX_ROWS as usize, 0),
            Err(Ref)
        ));
        assert!(matches!(
            CellReadPlan::relative(area, 0, MAX_COLUMNS as usize),
            Err(Ref)
        ));
    }
}
