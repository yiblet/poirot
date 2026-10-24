//! Budget-admitted copying of evaluated values across internal call boundaries.
use crate::{BudgetMeter, CellValue, EvalResult, EvaluationFailure, Value};

/// Clone a cell only after admitting its text storage.
pub(crate) fn copy_cell(budget: &mut BudgetMeter, value: &CellValue) -> EvalResult<CellValue> {
    if let Some(text) = value.text_ref() {
        budget.text(text.storage_bytes())?;
        budget.allocation(text.len_units())?;
    }
    Ok(value.clone())
}
/// Clone a value only after admitting its text, array, and reference storage.
pub(crate) fn copy_value(budget: &mut BudgetMeter, value: &Value) -> EvalResult<Value> {
    Ok(match value {
        Value::Missing => Value::Missing,
        Value::Scalar(value) => Value::Scalar(copy_cell(budget, value)?),
        Value::SelectedScalar(value, origin) => {
            Value::SelectedScalar(copy_cell(budget, value)?, *origin)
        }
        Value::SignedRefs(areas) => {
            budget.allocation(areas.len())?;
            Value::SignedRefs(areas.clone())
        }
        Value::Refs(areas) | Value::SheetSpan(areas) => {
            budget.allocation(areas.len())?;
            if matches!(value, Value::SheetSpan(_)) {
                Value::SheetSpan(areas.clone())
            } else {
                Value::Refs(areas.clone())
            }
        }
        Value::Array(rows) => {
            let cells = rows.iter().try_fold(0usize, |count, row| {
                count
                    .checked_add(row.len())
                    .ok_or(EvaluationFailure::StalePlan)
            })?;
            budget.allocation(cells)?;
            Value::Array(crate::ArrayView::anchored(
                rows.iter()
                    .map(|row| row.iter().map(|value| copy_cell(budget, value)).collect())
                    .collect::<EvalResult<_>>()?,
                rows.anchor,
            ))
        }
    })
}
