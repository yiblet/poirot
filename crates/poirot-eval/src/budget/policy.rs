//! Pure admission decisions; the meter owns attribution snapshots and mutation.
use super::{BudgetFailure, BudgetKind, EvaluationBudgets, EvaluationTarget, WorkKind};

/// Immutable consumed-work state used to decide whether a charge fits.
#[derive(Clone, Copy)]
pub(super) struct WorkSnapshot {
    /// Work already consumed by the run.
    pub run_consumed: u64,
    /// Direct work consumed by the active formula, when present.
    pub cell_consumed: Option<u64>,
    /// Cell attribution for rejected work.
    pub target: Option<EvaluationTarget>,
}
/// Proposed work charge, committed by the meter only after admission.
#[derive(Clone, Copy)]
pub(super) struct ChargeRequest {
    /// Usage counter category.
    pub kind: WorkKind,
    /// Additional work units requested.
    pub amount: u64,
}
/// Per-operation resource capacity checked separately from accumulated work.
#[derive(Clone, Copy)]
pub(super) enum Reservation {
    /// Logical collection element count.
    Values(u64),
    /// Text buffer storage bytes.
    TextBytes(usize),
    /// Wildcard state count for one match.
    WildcardStates(u64),
}

/// Reject configured limits beyond supported engine ceilings.
pub(super) fn validate_limits(limits: EvaluationBudgets) -> Result<(), BudgetFailure> {
    if limits.depth > 128 {
        return Err(BudgetFailure {
            kind: BudgetKind::Configuration,
            limit: 128,
            consumed: 0,
            requested: limits.depth as u64,
            target: None,
        });
    }
    Ok(())
}

/// Check a proposed charge against both remaining run and formula work.
pub(super) fn admit_work(
    limits: EvaluationBudgets,
    snapshot: WorkSnapshot,
    request: ChargeRequest,
) -> Result<ChargeRequest, BudgetFailure> {
    // Every category has the same admission rule; its counter is committed by the meter.
    let amount = request.amount;
    if amount > limits.run_work.saturating_sub(snapshot.run_consumed) {
        return Err(BudgetFailure {
            kind: BudgetKind::RunWork,
            limit: limits.run_work,
            consumed: snapshot.run_consumed,
            requested: amount,
            target: snapshot.target,
        });
    }
    if let Some(consumed) = snapshot.cell_consumed
        && amount > limits.cell_work.saturating_sub(consumed)
    {
        return Err(BudgetFailure {
            kind: BudgetKind::CellWork,
            limit: limits.cell_work,
            consumed,
            requested: amount,
            target: snapshot.target,
        });
    }
    Ok(request)
}

/// Check a resource size without mutating counters or allocating storage.
pub(super) fn admit_reservation(
    limits: EvaluationBudgets,
    target: Option<EvaluationTarget>,
    reservation: Reservation,
) -> Result<(), BudgetFailure> {
    let (kind, limit, requested, exceeded) = match reservation {
        Reservation::Values(count) => (
            BudgetKind::Materialization,
            limits.materialized_values as u64,
            count,
            count > limits.materialized_values as u64,
        ),
        Reservation::TextBytes(bytes) => (
            BudgetKind::TextBytes,
            limits.text_bytes as u64,
            bytes as u64,
            bytes > limits.text_bytes,
        ),
        Reservation::WildcardStates(states) => (
            BudgetKind::WildcardStates,
            limits.wildcard_states,
            states,
            states > limits.wildcard_states,
        ),
    };
    if exceeded {
        return Err(BudgetFailure {
            kind,
            limit,
            consumed: 0,
            requested,
            target,
        });
    }
    Ok(())
}
