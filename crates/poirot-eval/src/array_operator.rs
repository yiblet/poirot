//! Shared rectangular traversal and row-major operators; scalar kernels retain their policies.
use crate::*;

/// Logical operand dimensions, inspected without reading referenced cells.
#[derive(Clone, Copy)]
pub(super) struct Shape {
    pub(super) rows: usize,
    pub(super) columns: usize,
}
impl Shape {
    pub(super) fn inspect(value: &Value) -> Option<Self> {
        let (rows, columns) = match value {
            Value::Scalar(_) | Value::SelectedScalar(_, _) | Value::Missing => (1, 1),
            Value::Array(array) => {
                let columns = array.first()?.len();
                if columns == 0 || array.iter().any(|row| row.len() != columns) {
                    return None;
                }
                (array.len(), columns)
            }
            Value::Refs(areas) | Value::SheetSpan(areas) if areas.len() == 1 => {
                let area = *areas.first()?;
                (
                    usize::try_from(area.r1.checked_sub(area.r0)?.checked_add(1)?).ok()?,
                    usize::try_from(area.c1.checked_sub(area.c0)?.checked_add(1)?).ok()?,
                )
            }
            // SAFETY: The match guard checks areas.len() == 1 before reading its first element.
            Value::SignedRefs(areas) if areas.len() == 1 => areas[0].positive_shape()?,
            _ => return None,
        };
        Some(Self { rows, columns })
    }
}

/// Whether an operand needs array-mode selection rather than ordinary scalar projection.
pub(super) fn is_array_operand(value: &Value) -> bool {
    match value {
        Value::Array(_) => true,
        // SAFETY: Short-circuit evaluation reaches element zero only when areas.len() == 1.
        Value::Refs(areas) | Value::SheetSpan(areas) => areas.len() != 1 || areas[0].size() != 1,
        // SAFETY: Short-circuit evaluation reaches element zero only when areas.len() == 1.
        Value::SignedRefs(areas) => areas.len() != 1 || areas[0].size() != 1,
        Value::Scalar(_) | Value::SelectedScalar(_, _) | Value::Missing => false,
    }
}

impl Session<'_> {
    /// Apply a unary operator to each member without dereferencing unused coordinates.
    pub(super) fn array_unary(
        &mut self,
        sheet: usize,
        at: CellAddress,
        operation: UnaryOp,
        value: Value,
    ) -> EvalResult<Value> {
        let shape = Shape::inspect(&value).ok_or_else(|| {
            self.spill_unsupported(
                sheet,
                at,
                "array operator requires one bounded rectangular operand",
            )
        })?;
        self.rectangular_values(at, shape, |session, row, column| {
            let value = session.rectangular_element(&value, shape, row, column)?;
            let result = session.unary_value(sheet, at, operation, Value::Scalar(value))?;
            session.scalar(at, result)
        })
    }

    /// Broadcast singleton axes and execute both member reads before scalar error handling.
    pub(super) fn array_binary(
        &mut self,
        sheet: usize,
        at: CellAddress,
        operation: BinaryOp,
        left: Value,
        right: Value,
    ) -> EvalResult<Value> {
        let unsupported = || {
            self.spill_unsupported(
                sheet,
                at,
                "array operator requires single-sheet rectangular operands",
            )
        };
        let left_shape = Shape::inspect(&left).ok_or_else(unsupported)?;
        let right_shape = Shape::inspect(&right).ok_or_else(unsupported)?;
        let output = Shape {
            rows: left_shape.rows.max(right_shape.rows),
            columns: left_shape.columns.max(right_shape.columns),
        };
        self.rectangular_values(at, output, |session, row, column| {
            let left = session.rectangular_element(&left, left_shape, row, column)?;
            let right = session.rectangular_element(&right, right_shape, row, column)?;
            let result = session.binary_values(
                sheet,
                at,
                operation,
                Value::Scalar(left),
                Value::Scalar(right),
            )?;
            session.scalar(at, result)
        })
    }

    /// Materialize an array operation while preserving singleton array identity.
    pub(super) fn rectangular_values(
        &mut self,
        at: CellAddress,
        shape: Shape,
        element: impl FnMut(&mut Self, usize, usize) -> EvalResult<CellValue>,
    ) -> EvalResult<Value> {
        self.rectangular_array(at, shape, element).map(Value::Array)
    }

    /// Build an array without collapsing its singleton shape or reading unselected members.
    pub(super) fn rectangular_array(
        &mut self,
        at: CellAddress,
        shape: Shape,
        mut element: impl FnMut(&mut Self, usize, usize) -> EvalResult<CellValue>,
    ) -> EvalResult<ArrayView> {
        let count = (shape.rows as u64).saturating_mul(shape.columns as u64);
        self.budget.allocation_u64(count)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(shape.rows)
            .map_err(|_| EvaluationFailure::ResourceLimit {
                capability: CapabilityId::FunctionAdapter,
                source: self.consumer,
                detail: "rectangular row allocation failed".into(),
            })?;
        for row in 0..shape.rows {
            let mut values = Vec::new();
            values.try_reserve_exact(shape.columns).map_err(|_| {
                EvaluationFailure::ResourceLimit {
                    capability: CapabilityId::FunctionAdapter,
                    source: self.consumer,
                    detail: "rectangular member allocation failed".into(),
                }
            })?;
            for column in 0..shape.columns {
                // SAFETY: Reservation admitted shape.columns slots and this loop pushes exactly that many.
                values.push(element(self, row, column)?);
            }
            // SAFETY: Reservation admitted shape.rows slots and this loop pushes exactly that many.
            rows.push(values);
        }
        Ok(ArrayView::anchored(rows, Some(at)))
    }

    pub(super) fn rectangular_element(
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
            Value::Missing => return Ok(coerce_legacy_missing_blank()),
            Value::Scalar(value) | Value::SelectedScalar(value, _) => value,
            Value::Array(array) => array
                .get(row)
                .and_then(|values| values.get(column))
                .ok_or(EvaluationFailure::StalePlan)?,
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let area = *areas.first().ok_or(EvaluationFailure::StalePlan)?;
                return self.cell(
                    area.sheet,
                    CellAddress {
                        row: area.r0 + row as u32,
                        col: area.c0 + column as u32,
                    },
                );
            }
            Value::SignedRefs(areas) => {
                return self.signed_read(
                    areas
                        .first()
                        .ok_or(EvaluationFailure::StalePlan)?
                        .read(row as u64, column as u64)
                        .ok_or(EvaluationFailure::StalePlan)?,
                );
            }
        };
        self.budget.text(value.text_storage_bytes())?;
        Ok(value.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_capacity_overflow_is_an_attributed_resource_failure() {
        let workbook = Workbook::new();
        let target = EvaluationTarget {
            sheet: 0,
            address: "B2".parse().unwrap(),
        };
        // Each capacity exceeds Vec's byte limit, without requesting real heap storage.
        let shapes = [
            Shape {
                rows: usize::MAX / std::mem::size_of::<Vec<CellValue>>(),
                columns: 1,
            },
            Shape {
                rows: 1,
                columns: usize::MAX / std::mem::size_of::<CellValue>(),
            },
        ];
        for shape in shapes {
            let mut session = Session::with_context(&workbook, EvaluationInputs::default());
            session.consumer = Some(target);
            session.budget = BudgetMeter::new(EvaluationBudgets {
                run_work: u64::MAX,
                cell_work: u64::MAX,
                materialized_values: usize::MAX,
                ..Default::default()
            })
            .unwrap();
            let failure = session
                .rectangular_array(target.address, shape, |_, _, _| {
                    panic!("reservation must reject before evaluating any member")
                })
                .err()
                .unwrap();
            assert!(matches!(failure, EvaluationFailure::ResourceLimit {
                capability: CapabilityId::FunctionAdapter,
                source: Some(source), ..
            } if source == target));
            assert_eq!(
                session.budget.usage.materialized_values,
                (shape.rows * shape.columns) as u64
            );
        }
    }
}
