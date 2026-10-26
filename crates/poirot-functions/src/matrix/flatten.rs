//! Generic flattening geometry and filtering, independent of spreadsheet storage.
use crate::matrix::{Axis, Budget, Error, Matrix, Result};
use crate::scalar::CellError;

/// The order in which an input rectangle is traversed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanOrder {
    /// Visit columns within each row.
    Rows,
    /// Visit rows within each column.
    Columns,
}
/// The distinctions needed to filter a mixed-value array before normalization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberKind {
    /// An absent value, distinct from zero or empty text.
    Blank,
    /// An embedded spreadsheet error.
    Error,
    /// Any ordinary scalar value.
    Value,
}
/// Which original input members a flatten operation omits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ignore {
    /// Keep every input member.
    None,
    /// Omit only absent values.
    Blanks,
    /// Omit only spreadsheet errors.
    Errors,
    /// Omit both absent values and spreadsheet errors.
    BlanksAndErrors,
}
impl Ignore {
    /// Decide whether an original member survives filtering.
    pub fn retains(self, kind: MemberKind) -> bool {
        !matches!(
            (self, kind),
            (Self::Blanks | Self::BlanksAndErrors, MemberKind::Blank)
                | (Self::Errors | Self::BlanksAndErrors, MemberKind::Error)
        )
    }
}
/// Flatten retained members in the requested scan order into one row or column.
///
/// Classification precedes normalization: callers can distinguish absent values
/// from empty text. An empty result is a catchable calculation error.
pub fn flatten<T: Clone>(
    input: &Matrix<T>,
    axis: Axis,
    scan: ScanOrder,
    ignore: Ignore,
    classify: impl Fn(&T) -> MemberKind,
    budget: Budget,
) -> Result<Matrix<T>> {
    budget.check(Some(0), Some(input.values().len()))?;
    let count = input
        .values()
        .iter()
        .filter(|v| ignore.retains(classify(v)))
        .count();
    if count == 0 {
        return Err(Error::Formula(CellError::Calc));
    }
    budget.check(Some(count), input.values().len().checked_add(count))?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    let (outer, inner) = match scan {
        ScanOrder::Rows => (input.rows(), input.cols()),
        ScanOrder::Columns => (input.cols(), input.rows()),
    };
    for a in 0..outer {
        for b in 0..inner {
            let value = match scan {
                ScanOrder::Rows => input.get(a, b)?,
                ScanOrder::Columns => input.get(b, a)?,
            };
            if ignore.retains(classify(value)) {
                // A caller-provided classifier may use interior mutability. Refuse
                // inconsistent admissions rather than growing beyond the checked count.
                if values.len() >= count {
                    return Err(Error::Formula(CellError::Value));
                }
                values.push(value.clone());
            }
        }
    }
    let (rows, cols) = match axis {
        Axis::Rows => (1, count),
        Axis::Columns => (count, 1),
    };
    Matrix::new(rows, cols, values)
}
