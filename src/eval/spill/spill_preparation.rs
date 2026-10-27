use super::candidate_index::CandidateIndex;
use super::completion::Completion;
use super::held::Held;
use super::producer_candidate::ProducerCandidate;
use super::producer_read_evidence::ProducerReadEvidence;
use super::publication::Publication;
use super::settled_area::SettledArea;
use super::settlement_slot::SettlementSlot;
use crate::eval::coordinates::{CellMap, CellSet, EvaluationTarget};
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError, WorkKind};
use crate::eval::ports::{CellCompletion, CellObserver};
use crate::eval::reuse::SpillRole;
use crate::eval::values::Area;
use crate::model::CellAddress;

/// Tracks array discovery, provisional producer results, and settled spill ownership for a run.
/// Settlement keeps competing footprints private here until they can be admitted
/// and published without exposing a partially resolved component.
#[derive(Default)]
pub(in crate::eval) struct SpillPreparation {
    pub(super) failure: Option<EvaluationError>,
    pub(super) candidates: CandidateIndex,
    pub(super) settled: CellSet,
    pub(super) settlement: SettlementSlot,
    /// One fully settled range in this unchanged evaluation cohort.
    pub(super) ready: Option<SettledArea>,
    pub(super) held: CellMap<Held>,
    pub(super) computing: CellSet,
    /// Private producer observations with one recording and reset lifecycle.
    pub(in crate::eval) producer_reads: ProducerReadEvidence,
    /// Newly admitted or restored cells, acknowledged after incremental publication.
    pub(super) journal: Vec<(usize, CellAddress)>,
    /// Admissions can be provisional while a component is being settled.
    pub(super) roles: CellMap<SpillRole>,
}

impl SpillPreparation {
    /// Queue private-settlement observations or deliver them using only completion ports.
    pub(in crate::eval) fn complete_cell(
        &mut self,
        budget: &mut BudgetMeter,
        consumer: Option<EvaluationTarget>,
        observer: &mut Option<&mut dyn CellObserver>,
        completion: CellCompletion<'_>,
    ) -> EvalResult<()> {
        if observer.is_none() {
            return Ok(());
        }
        if let Some(active) = self.settlement.active_mut() {
            budget.allocation(1)?;
            if let Ok(value) = completion.result {
                budget.text(value.text_storage_bytes())?;
            }
            active
                .notifications
                .try_reserve(1)
                .map_err(|reservation_source| EvaluationError::Resource {
                    kind: crate::eval::limits::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    target: consumer,
                    reason: (crate::eval::limits::EvaluationReasonError::SpillNotificationAllocationFailed).with_reservation_source(reservation_source),
                })?;
            // SAFETY: The successful reservation above provides one completion slot.
            active.notifications.push(Completion {
                consumer: completion.consumer,
                target: completion.target,
                provenance: completion.provenance,
                result: completion.result.cloned().map_err(Clone::clone),
            });
            return Ok(());
        }
        if let Some(observer) = observer {
            observer.completed(completion)?;
        }
        Ok(())
    }

    /// Begin another evaluation identity while retaining empty observation buffers.
    pub(in crate::eval) fn reset(&mut self) {
        let mut producer_reads = std::mem::take(&mut self.producer_reads);
        producer_reads.clear();
        *self = Self {
            producer_reads,
            ..Self::default()
        };
    }

    /// Whether an earlier spill attempt has already aborted this evaluation.
    pub(in crate::eval) fn failed(&self) -> bool {
        self.failure.is_some()
    }

    /// Read an admitted role, which may still be provisional during settlement.
    pub(in crate::eval) fn admitted_role(&self, key: (usize, CellAddress)) -> SpillRole {
        self.roles.get(&key).copied().unwrap_or_default()
    }

    /// Whether this coordinate has an admitted generated role.
    pub(in crate::eval) fn has_admitted_role(&self, key: (usize, CellAddress)) -> bool {
        self.roles.contains_key(&key)
    }

    /// Borrow admitted coordinates without sorting, copying or publishing them.
    pub(in crate::eval) fn admitted_cells(
        &self,
    ) -> impl ExactSizeIterator<Item = &(usize, CellAddress)> {
        self.roles.keys()
    }

    /// Borrow publication evidence only when all private settlement has succeeded.
    pub(in crate::eval) fn publication(&self) -> EvalResult<Publication<'_>> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        if self.settlement.running || !self.held.is_empty() || !self.computing.is_empty() {
            return Err(EvaluationError::StalePlan);
        }
        Ok(Publication { preparation: self })
    }

    /// Determine losers from actual rectangles, in the explicit top-left priority order.
    pub(super) fn losers(
        &self,
        budget: &mut BudgetMeter,
        consumer: Option<EvaluationTarget>,
    ) -> EvalResult<CellSet> {
        let Some(active) = self.settlement.active() else {
            return Ok(CellSet::default());
        };
        let capacity = active.selected.len();
        budget.check_allocation(capacity)?;
        let mut accepted = Vec::new();
        accepted
            .try_reserve(capacity)
            .map_err(|reservation_source| EvaluationError::Resource {
                kind: crate::eval::limits::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                target: consumer,
                reason:
                    (crate::eval::limits::EvaluationReasonError::SpillArbitrationAllocationFailed)
                        .with_reservation_source(reservation_source),
            })?;
        let mut blocked = CellSet::default();
        for key in &active.selected {
            let Some(attempted) = active.footprints.get(key).copied() else {
                continue;
            };
            let area = attempted.area;
            budget.charge(
                WorkKind::ReferencePair,
                (accepted.len() as u64).saturating_add(1),
            )?;
            if attempted.settled_collision
                || accepted.iter().any(|other: &Area| {
                    ProducerCandidate {
                        anchor: CellAddress {
                            row: area.r0,
                            col: area.c0,
                        },
                        area,
                    }
                    .intersects(*other)
                })
            {
                blocked
                    .try_reserve(1)
                    .map_err(|reservation_source| EvaluationError::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        target: consumer,
                        reason: (crate::eval::limits::EvaluationReasonError::SpillBlockedOwnerAllocationFailed).with_reservation_source(reservation_source),
                    })?;
                // SAFETY: One slot was reserved locally; coordinate hashing is integer-only.
                blocked.insert(*key);
            } else {
                // SAFETY: Capacity reserves every selected producer, and each selected key is processed once.
                accepted.push(area);
            }
        }
        Ok(blocked)
    }

    /// Clear pending evidence only after the borrowed batch commits successfully.
    /// Callers must leave the journal intact when cache publication fails.
    pub(in crate::eval) fn acknowledge_publication(&mut self) {
        self.journal.clear();
    }
}
