//! Bulk traversal exists only for host contract regression tests.
use super::call_host::CallHost;
use super::scalar_operand::ScalarOperand;
use crate::eval::limits::EvaluationFailure;
use crate::eval::session::{ArgumentReader, ItemFolder};
use crate::eval::values::{FunctionFailure, FunctionResult, OperandOrigin, VALUE_ERROR, Value};

/// Bulk traversal modes retained for host contract regression tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TraversalPolicy {
    /// Visit only stored members under aggregation rules.
    SparseAggregation,
    /// Visit every logical position in the rectangle.
    DenseRectangle,
    /// Visit only the last row of an area operand.
    LastAreaRow,
}

impl TraversalPolicy {
    /// Materialize a test collection under this traversal contract.
    pub(super) fn collect(
        self,
        host: &mut CallHost<'_, '_>,
        index: usize,
    ) -> FunctionResult<Vec<ScalarOperand>> {
        let policy = self;
        let mut operand = host.argument(index)?;
        if policy == TraversalPolicy::SparseAggregation {
            let flag_count = operand.array_missing.iter().map(Vec::len).sum();
            let values = ItemFolder::new(host.session).fold_items(
                operand,
                Vec::new(),
                |values, additional| values.try_reserve(additional),
                |values, member| {
                    if values.len() >= values.capacity() {
                        return Err(EvaluationFailure::StalePlan);
                    }
                    // SAFETY: Capacity was checked for this member before the push.
                    values.push(member);
                    Ok(())
                },
            )?;
            host.check_values(values.len())?;
            host.check_values(flag_count)?;
            return Ok(values);
        }
        if policy == TraversalPolicy::LastAreaRow {
            match &mut operand.value {
                Value::Refs(areas) if areas.len() == 1 => areas[0].r0 = areas[0].r1,
                Value::SignedRefs(areas) if areas.len() == 1 => areas[0].r0 = areas[0].r1,
                Value::Refs(_) | Value::SheetSpan(_) | Value::SignedRefs(_) => {
                    return Err(VALUE_ERROR.into());
                }
                Value::Array(rows) => {
                    if let Some(row) = rows.values.pop() {
                        rows.values = vec![row];
                        if let Some(flags) = operand.array_missing.pop() {
                            operand.array_missing = vec![flags];
                        }
                    }
                }
                Value::Missing | Value::Scalar(_) | Value::SelectedScalar(_, _) => {}
            }
        }
        let origin = match operand.value {
            Value::Refs(_) | Value::SheetSpan(_) | Value::SignedRefs(_) => {
                OperandOrigin::ReferencedCell
            }
            Value::Array(_) => OperandOrigin::ArrayElement,
            Value::Missing | Value::Scalar(_) => OperandOrigin::Direct,
            Value::SelectedScalar(_, origin) => origin,
        };
        let values = ArgumentReader::new(host.session)
            .matrix(operand.value)?
            .map_err(FunctionFailure::Spreadsheet)?;
        host.check_values(values.iter().map(Vec::len).sum())?;
        Ok(values
            .into_iter()
            .enumerate()
            .flat_map(|(row, values)| {
                let flags = operand.array_missing.get(row);
                values
                    .into_iter()
                    .enumerate()
                    .map(move |(column, value)| ScalarOperand {
                        value,
                        origin,
                        missing: operand.missing
                            || flags
                                .and_then(|flags| flags.get(column))
                                .copied()
                                .unwrap_or(false),
                    })
            })
            .collect())
    }
}
