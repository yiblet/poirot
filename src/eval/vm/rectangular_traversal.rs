//! Rectangular array construction and broadcast member reads.

use super::shape::Shape;
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::session::{CellEvaluator, Session};
use crate::eval::values::{ArrayView, VALUE_ERROR, Value, convert_legacy_missing_blank};
use crate::model::CellValue::Error;
use crate::model::{CellAddress, CellValue};

/// Builds rectangular arrays member by member and reads broadcast operand members.
pub(in crate::eval) struct RectangularTraversal<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> RectangularTraversal<'s, 'a> {
    /// Borrow the session whose reads and budget traversal uses.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Materialize an array operation while preserving singleton array identity.
    pub(in crate::eval) fn values(
        &mut self,
        at: CellAddress,
        shape: Shape,
        element: impl FnMut(&mut Session<'a>, usize, usize) -> EvalResult<CellValue>,
    ) -> EvalResult<Value> {
        self.array(at, shape, element).map(Value::Array)
    }

    /// Build an array without collapsing its singleton shape or reading unselected members.
    pub(in crate::eval) fn array(
        &mut self,
        at: CellAddress,
        shape: Shape,
        mut element: impl FnMut(&mut Session<'a>, usize, usize) -> EvalResult<CellValue>,
    ) -> EvalResult<ArrayView> {
        let count = (shape.rows as u64).saturating_mul(shape.columns as u64);
        self.session.state.runtime.budget.allocation_u64(count)?;
        let mut rows = Vec::new();
        crate::eval::limits::reserve_exact(
            &mut rows,
            shape.rows,
            self.session.state.runtime.stack.consumer,
            CapabilityId::FunctionAdapter,
            "rectangular row allocation",
        )?;
        for row in 0..shape.rows {
            let mut values = Vec::new();
            crate::eval::limits::reserve_exact(
                &mut values,
                shape.columns,
                self.session.state.runtime.stack.consumer,
                CapabilityId::FunctionAdapter,
                "rectangular member allocation",
            )?;
            for column in 0..shape.columns {
                // SAFETY: Reservation admitted shape.columns slots and this loop pushes exactly that many.
                values.push(element(self.session, row, column)?);
            }
            // SAFETY: Reservation admitted shape.rows slots and this loop pushes exactly that many.
            rows.push(values);
        }
        Ok(ArrayView::anchored(rows, Some(at)))
    }

    /// Read one broadcast member, returning #VALUE! past an exhausted non-singleton axis.
    pub(in crate::eval) fn element(
        &mut self,
        value: &Value,
        shape: Shape,
        row: usize,
        column: usize,
    ) -> EvalResult<CellValue> {
        let row = if shape.rows == 1 { 0 } else { row };
        let column = if shape.columns == 1 { 0 } else { column };
        // POI's OperandResolver returns #VALUE! for exhausted non-singleton axes.
        if row >= shape.rows || column >= shape.columns {
            return Ok(Error(VALUE_ERROR));
        }
        let value = match value {
            // TODO(coercion-non-argument): [MEDIUM] Selecting a missing array operand as blank discards omission identity before its eventual consumer.
            Value::Missing => return Ok(convert_legacy_missing_blank()),
            Value::Scalar(value) | Value::SelectedScalar(value, _) => value,
            Value::Array(array) => array
                .get(row)
                .and_then(|values| values.get(column))
                .ok_or(EvaluationFailure::StalePlan)?,
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let area = *areas.first().ok_or(EvaluationFailure::StalePlan)?;
                return CellEvaluator::new(self.session).cell(
                    area.sheet,
                    CellAddress {
                        row: area.r0 + row as u32,
                        col: area.c0 + column as u32,
                    },
                );
            }
            Value::SignedRefs(areas) => {
                return CellEvaluator::new(self.session).signed_read(
                    areas
                        .first()
                        .ok_or(EvaluationFailure::StalePlan)?
                        .read(row as u64, column as u64)
                        .ok_or(EvaluationFailure::StalePlan)?,
                );
            }
        };
        self.session
            .state
            .runtime
            .budget
            .text(value.text_storage_bytes())?;
        Ok(value.clone())
    }
}
