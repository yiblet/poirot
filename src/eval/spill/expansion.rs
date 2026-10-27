use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError};
use crate::model::CellAddress;

/// Stages the uninspected source strips needed to extend a sheet's discovery proof.
/// New frontier endpoints replace the old ones only after scanning and storage
/// admission succeed, so a failure cannot mark unread cells as inspected.
pub(super) struct Expansion {
    pub(super) strips: Vec<crate::model::query::RangeMembership>,
    pub(super) removed_columns: Vec<u32>,
    pub(super) frontier_size: usize,
    pub(super) candidate_count: usize,
    /// None only for a new worksheet whose legacy saved owners are not inspected yet.
    pub(super) legacy: Option<std::sync::Arc<[CellAddress]>>,
}

impl Expansion {
    /// Retain one new disjoint source strip after fallible storage admission.
    pub(super) fn strip(
        &mut self,
        sheet: usize,
        first: CellAddress,
        last: CellAddress,
        budget: &mut BudgetMeter,
        consumer: Option<EvaluationTarget>,
    ) -> EvalResult<()> {
        budget.check_allocation(self.strips.len().saturating_add(1))?;
        self.strips
            .try_reserve(1)
            .map_err(|reservation_source| EvaluationError::Resource {
                kind: crate::eval::limits::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                target: consumer,
                reason: (crate::eval::limits::EvaluationReasonError::SpillDiscoveryStripAllocationFailed).with_reservation_source(reservation_source),
            })?;
        // SAFETY: The successful reservation provides one source strip slot.
        self.strips
            .push(crate::model::query::RangeMembership { sheet, first, last });
        Ok(())
    }
}
