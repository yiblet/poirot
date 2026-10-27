use super::self_read::SelfRead;
use crate::eval::coordinates::{CellMap, CellSet, EvaluationTarget};
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationFailure, WorkKind};
use crate::eval::values::Area;
use crate::model::CellAddress;

/// Records a producer's blank and sparse reads that may overlap its eventual array output.
/// Settlement uses this evidence to diagnose self-dependencies after the actual
/// footprint is known and clears it together with the producer's attempt.
#[derive(Default)]
pub(in crate::eval) struct ProducerReadEvidence {
    pub(super) blank_points: CellMap<CellSet>,
    pub(super) sparse_ranges: CellMap<Vec<Area>>,
}

impl ProducerReadEvidence {
    /// Record a blank observation against a producer's unpublished footprint.
    pub(in crate::eval) fn record_blank(
        &mut self,
        owner: (usize, CellAddress),
        point: (usize, CellAddress),
        budget: &mut BudgetMeter,
    ) -> EvalResult<()> {
        let previous = self.blank_points.get(&owner);
        if previous.is_some_and(|reads| reads.contains(&point)) {
            return Ok(());
        }
        budget.check_allocation(previous.map_or(0, CellSet::len).saturating_add(1))?;
        budget.charge(WorkKind::Materialization, 1)?;
        if previous.is_none() {
            self.blank_points
                .try_reserve(1)
                .map_err(|_| Self::allocation_failure(owner))?;
        }
        // SAFETY: A missing owner reserved one map entry above; an existing owner
        // adds no entry. Numeric coordinate hashing and equality cannot panic.
        let reads = self.blank_points.entry(owner).or_default();
        reads
            .try_reserve(1)
            .map_err(|_| Self::allocation_failure(owner))?;
        // SAFETY: The successful reservation admits this one new coordinate.
        reads.insert(point);
        Ok(())
    }

    /// Record a sparse range observation against a producer's unpublished footprint.
    pub(in crate::eval) fn record_sparse(
        &mut self,
        owner: (usize, CellAddress),
        area: Area,
        budget: &mut BudgetMeter,
    ) -> EvalResult<()> {
        let previous = self.sparse_ranges.get(&owner);
        budget.check_allocation(previous.map_or(0, Vec::len).saturating_add(1))?;
        budget.charge(WorkKind::Materialization, 1)?;
        if previous.is_none() {
            self.sparse_ranges
                .try_reserve(1)
                .map_err(|_| Self::allocation_failure(owner))?;
        }
        // SAFETY: A missing owner reserved one map entry above; an existing owner
        // adds no entry. Numeric coordinate hashing and equality cannot panic.
        let reads = self.sparse_ranges.entry(owner).or_default();
        reads
            .try_reserve(1)
            .map_err(|_| Self::allocation_failure(owner))?;
        // SAFETY: The successful reservation provides one range slot.
        reads.push(area);
        Ok(())
    }

    /// Attribute fallible observation storage to the producer being tracked.
    pub(super) fn allocation_failure(owner: (usize, CellAddress)) -> EvaluationFailure {
        EvaluationFailure::Resource {
            kind: crate::eval::limits::ResourceFailureKind::Allocation,
            capability: CapabilityId::FunctionAdapter,
            source: Some(EvaluationTarget {
                sheet: owner.0,
                address: owner.1,
            }),
            detail: "producer self-read observation allocation failed".into(),
        }
    }

    /// Classify a self-read while preserving sparse-footprint diagnostic priority.
    pub(super) fn self_read(
        &self,
        owner: (usize, CellAddress),
        point: (usize, CellAddress),
    ) -> Option<SelfRead> {
        if self.sparse_ranges.get(&owner).is_some_and(|ranges| {
            ranges
                .iter()
                .any(|area| area.sheet == point.0 && area.contains(point.1))
        }) {
            Some(SelfRead::Sparse)
        } else if self
            .blank_points
            .get(&owner)
            .is_some_and(|points| points.contains(&point))
        {
            Some(SelfRead::Blank)
        } else {
            None
        }
    }

    /// Forget producer-local observations before recomputation or after completion.
    pub(in crate::eval) fn forget(&mut self, owner: (usize, CellAddress)) {
        self.blank_points.remove(&owner);
        self.sparse_ranges.remove(&owner);
    }

    /// Reset all private self-read observations at a settlement or evaluation boundary.
    pub(in crate::eval) fn clear(&mut self) {
        self.blank_points.clear();
        self.sparse_ranges.clear();
    }
}
