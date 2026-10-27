use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::Value;
use crate::eval::vm::{RectangularTraversal, Shape};
use crate::model::CellValue;

/// Reads selected array members while the evaluator builds a rectangular result.
/// The restricted session access lets branch selection avoid reading unchosen coordinates.
pub(in crate::eval) struct SelectedReader<'read, 'workbook> {
    pub(super) session: &'read mut Session<'workbook>,
}

impl SelectedReader<'_, '_> {
    /// Read one selected branch member without evaluating unchosen coordinates.
    pub(in crate::eval) fn member(
        &mut self,
        value: &Value,
        shape: Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        RectangularTraversal::new(self.session).element(value, shape, row, column)
    }
}
