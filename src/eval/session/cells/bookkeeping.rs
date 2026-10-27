//! Cell bookkeeping helpers shared by the read lifecycle stages.

use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError};
use crate::model::CellValue;

/// Attribute exhausted bookkeeping storage to the cell whose attempt needed it.
pub(super) fn cell_allocation_failure(
    target: EvaluationTarget,
    reservation_source: std::collections::TryReserveError,
) -> EvaluationError {
    EvaluationError::Resource {
        kind: crate::eval::limits::ResourceFailureKind::Allocation,
        capability: CapabilityId::FunctionAdapter,
        target: Some(target),
        reason:
            crate::eval::limits::EvaluationReasonError::CellExecutionBookkeepingAllocationFailed
                .with_reservation_source(reservation_source),
    }
}

/// Copy a cached value, charging text in its own cell frame rather than its caller's.
#[inline]
pub(super) fn copy_cached_value(
    budget: &mut BudgetMeter,
    target: EvaluationTarget,
    value: &CellValue,
) -> EvalResult<CellValue> {
    // Nontext copies have no charge or rejection point requiring a cell frame.
    if let Some(text) = value.text_ref() {
        budget.push_cell(target);
        let result = budget.text(text.storage_bytes());
        budget.pop_cell();
        result?;
    }
    Ok(value.clone())
}
