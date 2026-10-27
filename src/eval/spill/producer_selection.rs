//! Settlement queueing of potential spill owners from inspected geometry.

use super::candidate_index::CandidateIndex;
use super::producer_candidate::ProducerCandidate;
use super::spill_preparation::SpillPreparation;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationFailure};
use crate::eval::values::Area;

/// Queues potential spill owners and their earlier-priority competitors for settlement.
/// It uses already inspected geometry; it does not recursively evaluate producers.
pub(super) struct ProducerSelection<'a> {
    pub(super) preparation: &'a mut SpillPreparation,
    pub(super) shared: Option<&'a std::cell::RefCell<CandidateIndex>>,
    pub(super) budget: &'a mut BudgetMeter,
    pub(super) consumer: Option<EvaluationTarget>,
}

impl ProducerSelection<'_> {
    /// Queue known direct owners or earlier competitors without recursive work.
    pub(super) fn select(
        &mut self,
        area: Area,
        competitor: Option<ProducerCandidate>,
        pending: &mut Vec<ProducerCandidate>,
    ) -> EvalResult<()> {
        let SpillPreparation {
            candidates,
            settled,
            settlement,
            ..
        } = self.preparation;
        let mut active = settlement.active_mut();
        let budget = &mut self.budget;
        let consumer = self.consumer;
        let mut select = |index: &CandidateIndex| -> EvalResult<()> {
            let discovery = index.get(&area.sheet).ok_or(EvaluationFailure::StalePlan)?;
            for candidate in discovery
                .possible_in(area, competitor.map(|c| c.anchor))
                .iter()
                .copied()
            {
                if !candidate.intersects(area) {
                    continue;
                }
                let key = (area.sheet, candidate.anchor);
                if settled.contains(&key)
                    || active.as_ref().is_none_or(|a| a.selected.contains(&key))
                {
                    continue;
                }
                budget.allocation(1)?;
                pending
                    .try_reserve(1)
                    .map_err(|_| EvaluationFailure::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        source: consumer,
                        detail: "spill frontier allocation failed".into(),
                    })?;
                if let Some(active) = active.as_mut() {
                    // SAFETY: Integer coordinate ordering cannot panic; each
                    // B-tree allocation has a fixed-size representable node layout.
                    active.selected.insert(key);
                }
                // SAFETY: The successful reservation above provides one queue slot.
                pending.push(candidate);
            }
            Ok(())
        };
        if let Some(shared) = self.shared {
            let index = shared
                .try_borrow()
                .map_err(|_| EvaluationFailure::StalePlan)?;
            select(&index)
        } else {
            select(candidates)
        }
    }
}
