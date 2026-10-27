//! Select scalar positions without reading cells or consuming budgets.
use crate::eval::values::VALUE_ERROR;
use crate::model::{CellAddress, CellError};

/// Select array offsets without reading values, preserving unknown-anchor failures.
pub(super) fn array_position(
    view: &crate::eval::values::ArrayView,
    at: CellAddress,
) -> Result<Result<(usize, usize), CellError>, crate::eval::limits::EvaluationReasonError> {
    let height = view.len();
    let width = view.first().map_or(0, Vec::len);
    if height == 1 && width == 1 {
        return Ok(Ok((0, 0)));
    }
    let anchor = view
        .anchor
        .ok_or(crate::eval::limits::EvaluationReasonError::ComputedArrayScalarCoordinatesAreNotEstablishedByItsAdapter)?;
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
