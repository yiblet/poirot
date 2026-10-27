//! Source-only candidate discovery with atomic completeness publication.
use super::candidate_index::CandidateIndex;
use super::expansion::Expansion;
use super::producer_candidate::ProducerCandidate;
use super::sheet_candidates::SheetCandidates;
use crate::eval::compile::compiler::CompiledWorkbook;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError, WorkKind};
use crate::model::{CellAddress, Workbook};

/// Discovers possible array producers from authored cells and compiled shape bounds.
/// It records which source regions were fully inspected so settlement can avoid
/// repeated scans without executing formulas during discovery.
pub(super) struct Discovery<'a, 's> {
    pub(super) workbook: &'a Workbook,
    pub(super) inputs: Option<&'a crate::eval::inputs::RunInputs>,
    pub(super) plan: Option<&'a CompiledWorkbook>,
    pub(super) source_preparation: crate::eval::compile::SourcePreparation<'a>,
    pub(super) shared: Option<&'a std::cell::RefCell<CandidateIndex>>,
    pub(super) local: &'s mut CandidateIndex,
    pub(super) budget: &'s mut BudgetMeter,
    pub(super) consumer: Option<EvaluationTarget>,
}

impl Discovery<'_, '_> {
    /// Inspect only new origin-prefix strips and publish completeness atomically.
    pub(super) fn discover(&mut self, sheet: usize, last: CellAddress) -> EvalResult<()> {
        last.validate().map_err(|_| EvaluationError::StalePlan)?;
        let consumer = self.consumer;
        let plan =
            |index: &CandidateIndex, budget: &mut BudgetMeter| -> EvalResult<Option<Expansion>> {
                if let Some(discovery) = index.get(&sheet) {
                    discovery.expansion(sheet, last, budget, consumer)
                } else {
                    let mut expansion =
                        SheetCandidates::default().expansion(sheet, last, budget, consumer)?;
                    if let Some(expansion) = &mut expansion {
                        expansion.legacy = None;
                    }
                    Ok(expansion)
                }
            };
        let expansion = if let Some(shared) = self.shared {
            let index = shared
                .try_borrow()
                .map_err(|_| EvaluationError::StalePlan)?;
            plan(&index, self.budget)?
        } else {
            plan(self.local, self.budget)?
        };
        let Some(Expansion {
            strips,
            removed_columns,
            frontier_size,
            candidate_count,
            legacy,
        }) = expansion
        else {
            return Ok(());
        };
        let legacy = if let Some(legacy) = legacy {
            legacy
        } else {
            let mut unique = std::collections::HashSet::new();
            if let Some(worksheet) = self.workbook.sheet(sheet) {
                for (_, owner) in worksheet.spill_children() {
                    self.budget.charge(WorkKind::SparseScan, 1)?;
                    if worksheet.spill_extent(owner).is_none() && !unique.contains(&owner) {
                        self.budget
                            .check_allocation(unique.len().saturating_add(1))?;
                        unique
                            .try_reserve(1)
                            .map_err(|reservation_source| EvaluationError::Resource {
                                kind: crate::eval::limits::ResourceFailureKind::Allocation,
                                capability: CapabilityId::FunctionAdapter,
                                target: self.consumer,
                                reason: (crate::eval::limits::EvaluationReasonError::LegacySpillOwnerAllocationFailed).with_reservation_source(reservation_source),
                            })?;
                        // SAFETY: The successful reservation above provides one owner slot.
                        unique.insert(owner);
                    }
                }
            }
            let mut owners = Vec::new();
            owners
                .try_reserve(unique.len())
                .map_err(|reservation_source| EvaluationError::Resource {
                    kind: crate::eval::limits::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    target: self.consumer,
                    reason: (crate::eval::limits::EvaluationReasonError::LegacySpillOwnerOrderingAllocationFailed).with_reservation_source(reservation_source),
                })?;
            for owner in unique {
                // SAFETY: Reservation above covers every unique owner, moved exactly once.
                owners.push(owner);
            }
            owners.sort_unstable();
            let _shared_bytes = owners
                .len()
                .checked_mul(std::mem::size_of::<CellAddress>())
                .and_then(|bytes| bytes.checked_add(2 * std::mem::size_of::<usize>()))
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .ok_or( EvaluationError::Resource {
                    kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
                    capability: CapabilityId::FunctionAdapter,
                    target: self.consumer,
                    reason: crate::eval::limits::EvaluationReasonError::LegacySpillOwnerSharedAllocationOverflow,
                })?;
            // SAFETY: The checked bytes include the Arc reference-count header;
            // CellAddress and usize have compatible alignment and representable layout.
            std::sync::Arc::from(owners)
        };
        let effective = crate::eval::inputs::EffectiveWorkbook::new(self.workbook, self.inputs);
        let mut added = Vec::new();
        for membership in strips {
            for (&at, cell) in effective
                .stored(membership)
                .map_err(|_| EvaluationError::StalePlan)?
            {
                self.budget.charge(WorkKind::SparseScan, 1)?;
                if let Some(source) = cell.formula.as_deref()
                    && let Some(shape) = self
                        .source_preparation
                        .spill_shape(self.plan, sheet, at, source)
                {
                    self.budget
                        .check_allocation(added.len().saturating_add(1))?;
                    added
                        .try_reserve(1)
                        .map_err(|reservation_source| EvaluationError::Resource {
                            kind: crate::eval::limits::ResourceFailureKind::Allocation,
                            capability: CapabilityId::FunctionAdapter,
                            target: self.consumer,
                            reason: (crate::eval::limits::EvaluationReasonError::SpillCandidateAllocationFailed).with_reservation_source(reservation_source),
                        })?;
                    let mut candidate = ProducerCandidate::new(sheet, at, shape);
                    // Saved geometry is independent of current computed shape.
                    if let Some(worksheet) = self.workbook.sheet(sheet)
                        && let Some(last) = worksheet.spill_extent(at)
                    {
                        candidate.area.r1 = candidate.area.r1.max(last.row);
                        candidate.area.c1 = candidate.area.c1.max(last.col);
                    }
                    if legacy.binary_search(&at).is_ok() {
                        candidate =
                            ProducerCandidate::new(sheet, at, crate::model::ShapeBounds::UNKNOWN);
                    }
                    // SAFETY: The successful reservation above provides one candidate slot.
                    added.push(candidate);
                }
            }
        }
        self.budget
            .check_allocation(added.len().saturating_add(candidate_count))?;
        added.sort_unstable_by_key(|c| c.anchor);
        self.budget.check_allocation(frontier_size)?;
        let consumer = self.consumer;
        let commit = |index: &mut CandidateIndex| -> EvalResult<()> {
            let update = |discovery: &mut SheetCandidates| -> EvalResult<()> {
                discovery.candidates.try_reserve(added.len()).map_err(|reservation_source| {
                    EvaluationError::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        target: consumer,
                        reason: (crate::eval::limits::EvaluationReasonError::SpillCandidateAppendAllocationFailed).with_reservation_source(reservation_source),
                    }
                })?;
                // The successful candidate reservation admits this complete count.
                let summary_len = discovery.candidates.len().saturating_add(added.len());
                let summary_additional = summary_len.saturating_sub(discovery.ending_rows.len());
                discovery
                    .ending_rows
                    .try_reserve(summary_additional)
                    .map_err(|reservation_source| EvaluationError::Resource {
                        kind: crate::eval::limits::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        target: consumer,
                        reason: (crate::eval::limits::EvaluationReasonError::SpillCandidateRowSummaryAllocationFailed).with_reservation_source(reservation_source),
                    })?;
                let interleaved = discovery
                    .candidates
                    .last()
                    .zip(added.first())
                    .is_some_and(|(old, new)| old.anchor > new.anchor);
                // SAFETY: The successful reservation admits every staged candidate.
                discovery.candidates.extend_from_slice(&added);
                if interleaved {
                    discovery.candidates.sort_unstable_by_key(|c| c.anchor);
                    discovery.ending_rows.clear();
                }
                let mut ending_row = discovery.ending_rows.last().copied().unwrap_or(0);
                for candidate in discovery
                    .candidates
                    .iter()
                    .skip(discovery.ending_rows.len())
                {
                    ending_row = ending_row.max(candidate.area.r1);
                    // SAFETY: The reservation covers the final candidate count.
                    // Iteration pushes at most that many entries; clear retains capacity.
                    discovery.ending_rows.push(ending_row);
                }
                discovery.legacy = legacy;
                // Coverage changes only after all scanned candidates are safely retained.
                for column in removed_columns {
                    discovery.inspected.remove(&column);
                }
                // SAFETY: Integer ordering cannot panic; each B-tree allocation
                // contains a fixed-size node with a representable layout.
                discovery.inspected.insert(last.col, last.row);
                Ok(())
            };
            if let Some(discovery) = index.get_mut(&sheet) {
                update(discovery)
            } else {
                let mut discovery = SheetCandidates::default();
                update(&mut discovery)?;
                // SAFETY: Integer ordering cannot panic; each B-tree allocation
                // contains a fixed-size node with a representable layout.
                index.insert(sheet, discovery);
                Ok(())
            }
        };
        if let Some(shared) = self.shared {
            let mut index = shared
                .try_borrow_mut()
                .map_err(|_| EvaluationError::StalePlan)?;
            commit(&mut index)
        } else {
            commit(self.local)
        }
    }
}
