//! Pure scalar selection; the policy never reads cells or consumes budgets.
use crate::eval::values::VALUE_ERROR;
use crate::model::{CellAddress, CellError};

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
        view: &crate::eval::values::ArrayView,
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
