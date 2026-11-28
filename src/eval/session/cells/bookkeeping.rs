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

/// Count one retained result lookup after its cell read was admitted.
#[inline]
pub(super) fn record_external_hit(runtime: &mut crate::eval::session::runtime::Runtime) {
    if runtime.budget.profiling {
        runtime.run_usage.external_cache_hits =
            runtime.run_usage.external_cache_hits.saturating_add(1);
        runtime.budget.usage.cache_hits = runtime.budget.usage.cache_hits.saturating_add(1);
    }
}

/// Count an admitted read served by the local memo.
#[inline]
pub(super) fn record_memo_hit(budget: &mut BudgetMeter, usage: &mut crate::eval::limits::RunUsage) {
    if budget.profiling {
        usage.session_memo_hits = usage.session_memo_hits.saturating_add(1);
        budget.usage.cache_hits = budget.usage.cache_hits.saturating_add(1);
    }
}

/// Stage a successful point result for atomic overlay publication.
#[inline]
pub(super) fn record_overlay_completion(
    reads: &mut crate::eval::reuse::Reads,
    target: EvaluationTarget,
    provenance: crate::eval::ports::CellProvenance,
) -> EvalResult<()> {
    let key = (target.sheet, target.address);
    reads
        .completed
        .try_reserve(1)
        .map_err(|source| cell_allocation_failure(target, source))?;
    if provenance == crate::eval::ports::CellProvenance::Evaluated {
        reads
            .fresh_results
            .try_reserve(1)
            .map_err(|source| cell_allocation_failure(target, source))?;
        // SAFETY: One slot was reserved and its key hashes only integer coordinates.
        reads.fresh_results.insert(key);
    }
    // SAFETY: One slot was reserved before any write to the completed set.
    reads.completed.insert(key);
    Ok(())
}
