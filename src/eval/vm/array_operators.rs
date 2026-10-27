//! Row-major array operators; scalar kernels retain their policies.

use super::operators::Operators;
use super::rectangular_traversal::RectangularTraversal;
use super::shape::Shape;
use crate::eval::limits::EvalResult;
use crate::eval::session::{ResultProjection, Session};
use crate::eval::values::Value;
use crate::model::CellAddress;
use poirot_parsing::formula::{BinaryOp, UnaryOp};

/// Applies unary and binary operators member-wise over rectangular operands.
pub(in crate::eval) struct ArrayOperators<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ArrayOperators<'s, 'a> {
    /// Borrow the session whose reads and budget array operators use.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Apply a unary operator to each member without dereferencing unused coordinates.
    pub(in crate::eval) fn array_unary(
        &mut self,
        sheet: usize,
        at: CellAddress,
        operation: UnaryOp,
        value: Value,
    ) -> EvalResult<Value> {
        let shape = Shape::inspect(&value).ok_or_else(|| {
            crate::eval::spill::unsupported(
                sheet,
                at,
                crate::eval::limits::EvaluationReasonError::ArrayOperatorRequiresOneBoundedRectangularOperand,
            )
        })?;
        RectangularTraversal::new(self.session).values(at, shape, |session, row, column| {
            let value = RectangularTraversal::new(session).element(&value, shape, row, column)?;
            let result =
                Operators::new(session).unary_value(sheet, at, operation, Value::Scalar(value))?;
            ResultProjection::new(session).select_legacy_scalar(at, result)
        })
    }

    /// Broadcast singleton axes and execute both member reads before scalar error handling.
    pub(in crate::eval) fn array_binary(
        &mut self,
        sheet: usize,
        at: CellAddress,
        operation: BinaryOp,
        left: Value,
        right: Value,
    ) -> EvalResult<Value> {
        let unsupported = || {
            crate::eval::spill::unsupported(
                sheet,
                at,
                crate::eval::limits::EvaluationReasonError::ArrayOperatorRequiresSingleSheetRectangularOperands,
            )
        };
        let left_shape = Shape::inspect(&left).ok_or_else(unsupported)?;
        let right_shape = Shape::inspect(&right).ok_or_else(unsupported)?;
        let output = Shape {
            rows: left_shape.rows.max(right_shape.rows),
            columns: left_shape.columns.max(right_shape.columns),
        };
        RectangularTraversal::new(self.session).values(at, output, |session, row, column| {
            let left =
                RectangularTraversal::new(session).element(&left, left_shape, row, column)?;
            let right =
                RectangularTraversal::new(session).element(&right, right_shape, row, column)?;
            let result = Operators::new(session).scalar_binary_values(operation, left, right)?;
            ResultProjection::new(session).select_legacy_scalar(at, result)
        })
    }
}
