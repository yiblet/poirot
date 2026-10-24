//! Dynamic array ownership and atomic result publication.
//!
//! `discovery::Sources` reads authored cells and shape bounds without executing
//! formulas. `settlement::Engine` owns component selection, retries and rollback
//! through an explicit execution port. `admission::Admission` publishes memoized
//! results using only storage and resource capabilities. This module owns their
//! shared preparation state and the Session entry points; `execution` attaches
//! recursive formula execution without giving the engine evaluator-wide access.
use super::*;

mod admission;
mod discovery;
mod execution;
mod settlement;

/// Worksheet source-inspection proofs and possible rectangles under one preparation identity.
pub(super) type CandidateIndex = std::collections::BTreeMap<usize, Discovery>;
/// Source classification and the exact union of inspected origin prefixes.
#[derive(Default)]
pub(super) struct Discovery {
    candidates: Vec<Candidate>,
    /// Monotone maximum possible ending row through each ordered candidate.
    ending_rows: Vec<u32>,
    /// Maximal prefix endpoints keyed by column; rows strictly decrease.
    inspected: std::collections::BTreeMap<u32, u32>,
    /// Saved children without declared geometry cannot supply finite bounds.
    legacy: std::sync::Arc<[CellAddress]>,
}
impl Discovery {
    /// Whether every authored coordinate in this origin prefix was inspected successfully.
    fn covers(&self, last: CellAddress) -> bool {
        self.inspected
            // SAFETY: With no upper bound, this included lower bound cannot be reversed.
            .range(last.col..)
            .next()
            .is_some_and(|(_, row)| *row >= last.row)
    }
    /// Bound candidate tests by possible ending rows and source-priority starts.
    fn possible_in(&self, area: Area, before: Option<CellAddress>) -> &[Candidate] {
        let end = match before {
            Some(anchor) => self.candidates.partition_point(|c| c.anchor < anchor),
            None => self.candidates.partition_point(|c| c.anchor.row <= area.r1),
        };
        let start = self
            .ending_rows
            .partition_point(|row| *row < area.r0)
            .min(end);
        self.candidates.get(start..end).unwrap_or_default()
    }
    /// Stage only uncovered bands and dominated endpoints, without changing the proof.
    fn expansion(
        &self,
        sheet: usize,
        last: CellAddress,
        budget: &mut BudgetMeter,
        consumer: Option<EvaluationTarget>,
    ) -> EvalResult<Option<Expansion>> {
        // SAFETY: An unbounded upper endpoint cannot precede this included lower endpoint.
        let boundary = self.inspected.range(last.col..).next();
        let covered_row = boundary.map(|(_, row)| *row);
        let replaces_column = boundary.is_some_and(|(column, _)| *column == last.col);
        if covered_row.is_some_and(|row| row >= last.row) {
            return Ok(None);
        }
        let mut expansion = Expansion {
            strips: Vec::new(),
            removed_columns: Vec::new(),
            frontier_size: 0,
            candidate_count: self.candidates.len(),
            legacy: Some(self.legacy.clone()),
        };
        let mut row = covered_row.map_or(0, |row| row.saturating_add(1));
        // Rows increase in this reverse column traversal. Every visited endpoint
        // except at most the final boundary is dominated by the new prefix.
        // SAFETY: With no lower bound, this included upper bound cannot be reversed.
        for (&column, &end_row) in self.inspected.range(..=last.col).rev() {
            if end_row >= row && column < last.col {
                expansion.strip(
                    sheet,
                    CellAddress {
                        row,
                        col: column.saturating_add(1),
                    },
                    CellAddress {
                        row: end_row.min(last.row),
                        col: last.col,
                    },
                    budget,
                    consumer,
                )?;
            }
            if end_row <= last.row && column != last.col {
                budget.check_allocation(expansion.removed_columns.len().saturating_add(1))?;
                expansion.removed_columns.try_reserve(1).map_err(|_| {
                    EvaluationFailure::Resource {
                        kind: crate::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        source: consumer,
                        detail: "spill frontier replacement allocation failed".into(),
                    }
                })?;
                // SAFETY: The successful reservation provides one dominated endpoint slot.
                expansion.removed_columns.push(column);
            }
            row = row.max(end_row.min(last.row).saturating_add(1));
            if row > last.row {
                break;
            }
        }
        if row <= last.row {
            expansion.strip(sheet, CellAddress { row, col: 0 }, last, budget, consumer)?;
        }
        expansion.frontier_size = self
            .inspected
            .len()
            .saturating_sub(expansion.removed_columns.len())
            .saturating_add(usize::from(!replaces_column));
        Ok(Some(expansion))
    }
}
/// Private new source bands and frontier replacements admitted before publication.
struct Expansion {
    strips: Vec<poirot_model::query::RangeMembership>,
    removed_columns: Vec<u32>,
    frontier_size: usize,
    candidate_count: usize,
    /// None only for a new worksheet whose legacy saved owners are not inspected yet.
    legacy: Option<std::sync::Arc<[CellAddress]>>,
}
impl Expansion {
    /// Retain one new disjoint source strip after fallible storage admission.
    fn strip(
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
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: consumer,
                detail: "spill discovery strip allocation failed".into(),
            })?;
        // SAFETY: The successful reservation provides one source strip slot.
        self.strips
            .push(poirot_model::query::RangeMembership { sheet, first, last });
        Ok(())
    }
}
/// Source-owned upper rectangle; actual admission alone establishes ownership.
#[derive(Clone, Copy)]
pub(super) struct Candidate {
    /// Authored formula coordinate and deterministic scheduling key.
    pub anchor: CellAddress,
    /// Conservative possible footprint clipped to the worksheet grid.
    pub area: Area,
}
impl Candidate {
    /// Place independent type bounds down/right from a valid authored coordinate.
    fn new(sheet: usize, anchor: CellAddress, shape: poirot_model::ShapeBounds) -> Self {
        let last = |start: u32, bound: Option<std::num::NonZeroU32>, limit: u32| {
            bound.map_or(limit.saturating_sub(1), |n| {
                start
                    .saturating_add(n.get().saturating_sub(1))
                    .min(limit.saturating_sub(1))
            })
        };
        Self {
            anchor,
            area: Area {
                kind: AreaKind::Area,
                sheet,
                r0: anchor.row,
                c0: anchor.col,
                r1: last(anchor.row, shape.rows, poirot_model::MAX_ROWS),
                c1: last(anchor.col, shape.columns, poirot_model::MAX_COLUMNS),
            },
        }
    }
    /// Whether a demanded physical rectangle can observe this producer.
    fn intersects(self, area: Area) -> bool {
        self.area.sheet == area.sheet
            && self.area.r0 <= area.r1
            && area.r0 <= self.area.r1
            && self.area.c0 <= area.c1
            && area.c0 <= self.area.c1
    }
}
/// Demand-local discovery, private settlement and admitted generated ownership.
#[derive(Default)]
pub(super) struct Preparation {
    failure: Option<EvaluationFailure>,
    candidates: CandidateIndex,
    settled: CellSet,
    settlement: SettlementSlot,
    /// One fully settled range in this unchanged evaluation cohort.
    ready: Option<SettledArea>,
    held: CellMap<Held>,
    computing: CellSet,
    /// Private producer observations with one recording and reset lifecycle.
    pub(super) producer_reads: ProducerReadEvidence,
    /// Newly admitted or restored cells, acknowledged after incremental publication.
    journal: Vec<(usize, CellAddress)>,
    /// Admissions can be provisional while a component is being settled.
    roles: CellMap<SpillRole>,
}
impl Preparation {
    /// Queue private-settlement observations or deliver them using only completion ports.
    pub(super) fn complete_cell(
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
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: consumer,
                    detail: "spill notification allocation failed".into(),
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
    pub(super) fn reset(&mut self) {
        let mut producer_reads = std::mem::take(&mut self.producer_reads);
        producer_reads.clear();
        *self = Self {
            producer_reads,
            ..Self::default()
        };
    }
    /// Whether an earlier spill attempt has already aborted this evaluation.
    pub(super) fn failed(&self) -> bool {
        self.failure.is_some()
    }
    /// Read an admitted role, which may still be provisional during settlement.
    pub(super) fn admitted_role(&self, key: (usize, CellAddress)) -> SpillRole {
        self.roles.get(&key).copied().unwrap_or_default()
    }
    /// Whether this coordinate has an admitted generated role.
    pub(super) fn has_admitted_role(&self, key: (usize, CellAddress)) -> bool {
        self.roles.contains_key(&key)
    }
    /// Borrow admitted coordinates without sorting, copying or publishing them.
    pub(super) fn admitted_cells(&self) -> impl ExactSizeIterator<Item = &(usize, CellAddress)> {
        self.roles.keys()
    }
    /// Borrow publication evidence only when all private settlement has succeeded.
    pub(super) fn publication(&self) -> EvalResult<Publication<'_>> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        if self.settlement.running || !self.held.is_empty() || !self.computing.is_empty() {
            return Err(EvaluationFailure::StalePlan);
        }
        Ok(Publication { preparation: self })
    }
    /// Determine losers from actual rectangles, in the explicit top-left priority order.
    fn losers(
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
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: consumer,
                detail: "spill arbitration allocation failed".into(),
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
                    Candidate {
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
                    .map_err(|_| EvaluationFailure::Resource {
                        kind: crate::ResourceFailureKind::Allocation,
                        capability: CapabilityId::FunctionAdapter,
                        source: consumer,
                        detail: "spill blocked-owner allocation failed".into(),
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
    pub(super) fn acknowledge_publication(&mut self) {
        self.journal.clear();
    }
}

/// Borrowed evidence whose construction excludes failed or active private settlement.
pub(super) struct Publication<'a> {
    preparation: &'a Preparation,
}
impl Publication<'_> {
    /// Count settled generated coordinates without visiting their values.
    pub(super) fn generated_count(&self) -> usize {
        self.preparation.roles.len()
    }
    /// Borrow settled generated children with their producers.
    pub(super) fn children(
        &self,
    ) -> impl Iterator<Item = ((usize, CellAddress), CellAddress)> + '_ {
        self.preparation
            .roles
            .iter()
            .filter_map(|(&key, role)| match role {
                SpillRole::Child { anchor } => Some((key, *anchor)),
                _ => None,
            })
    }
    /// Read a settled producer's ending coordinate.
    pub(super) fn extent(&self, key: (usize, CellAddress)) -> Option<CellAddress> {
        self.role(key).last()
    }
    /// Newly admitted or restored coordinates awaiting a successful cache commit.
    pub(super) fn pending_cells(&self) -> &[(usize, CellAddress)] {
        &self.preparation.journal
    }
    /// Read the settled role belonging to a coordinate in this evaluation.
    pub(super) fn role(&self, key: (usize, CellAddress)) -> SpillRole {
        self.preparation.admitted_role(key)
    }
    /// Whether a coordinate belongs to the settled generated footprint.
    pub(super) fn contains(&self, key: (usize, CellAddress)) -> bool {
        self.preparation.has_admitted_role(key)
    }
}
/// Producer self-read observations with coordinated recording, diagnosis and reset.
/// Separate physical maps avoid unused per-producer container storage.
#[derive(Default)]
pub(super) struct ProducerReadEvidence {
    blank_points: CellMap<CellSet>,
    sparse_ranges: CellMap<Vec<Area>>,
}
impl ProducerReadEvidence {
    /// Record a blank observation against a producer's unpublished footprint.
    pub(super) fn record_blank(
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
    pub(super) fn record_sparse(
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
    fn allocation_failure(owner: (usize, CellAddress)) -> EvaluationFailure {
        EvaluationFailure::Resource {
            kind: crate::ResourceFailureKind::Allocation,
            capability: CapabilityId::FunctionAdapter,
            source: Some(EvaluationTarget {
                sheet: owner.0,
                address: owner.1,
            }),
            detail: "producer self-read observation allocation failed".into(),
        }
    }
    /// Classify a self-read while preserving sparse-footprint diagnostic priority.
    fn self_read(
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
    pub(super) fn forget(&mut self, owner: (usize, CellAddress)) {
        self.blank_points.remove(&owner);
        self.sparse_ranges.remove(&owner);
    }
    /// Reset all private self-read observations at a settlement or evaluation boundary.
    pub(super) fn clear(&mut self) {
        self.blank_points.clear();
        self.sparse_ranges.clear();
    }
}
/// The observation responsible for an unpublished-footprint dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelfRead {
    Blank,
    Sparse,
}

/// A physical operand whose source discovery and ownership settlement both completed.
#[derive(Clone, Copy)]
struct SettledArea(Area);
/// One lazily allocated component workspace, active only during private settlement.
#[derive(Default)]
struct SettlementSlot {
    workspace: Option<Box<Settlement>>,
    running: bool,
}
impl SettlementSlot {
    /// Read semantic attempt state only while its component is running.
    fn active(&self) -> Option<&Settlement> {
        self.workspace.as_deref().filter(|_| self.running)
    }
    /// Mutate semantic attempt state without exposing retained inactive buffers.
    fn active_mut(&mut self) -> Option<&mut Settlement> {
        self.workspace.as_deref_mut().filter(|_| self.running)
    }
    /// Begin a new component using empty collections left by its predecessor.
    fn begin(
        &mut self,
        memo_usage: run::MemoRetentionUsage,
        journal_len: usize,
        admitted_len: usize,
    ) {
        let state = self.workspace.get_or_insert_with(|| {
            // SAFETY: One fixed-size Settlement cannot overflow an allocation layout.
            Box::new(Settlement {
                selected: Default::default(),
                blocked: Default::default(),
                footprints: Default::default(),
                admitted: Default::default(),
                consumers: Default::default(),
                random_touched: Default::default(),
                memo_usage: Default::default(),
                journal_len: 0,
                admitted_len: 0,
                notifications: Vec::new(),
                non_repeatable: false,
            })
        });
        state.memo_usage = memo_usage;
        state.journal_len = journal_len;
        state.admitted_len = admitted_len;
        self.running = true;
    }
    /// Discard attempt contents on success or failure while retaining reusable storage.
    fn finish(&mut self) {
        self.running = false;
        if let Some(state) = self.workspace.as_deref_mut() {
            // Hash tables and the notification vector retain capacities; BTreeSet
            // clears its nodes. No selected owner or old replay effect survives.
            state.selected.clear();
            state.blocked.clear();
            state.footprints.clear();
            state.admitted.clear();
            state.consumers.clear();
            state.random_touched.clear();
            state.notifications.clear();
            state.memo_usage = Default::default();
            state.journal_len = 0;
            state.admitted_len = 0;
            state.non_repeatable = false;
        }
    }
}
/// One private component attempt; prior public results remain outside its rollback set.
struct Settlement {
    selected: std::collections::BTreeSet<(usize, CellAddress)>,
    blocked: CellSet,
    footprints: CellMap<AttemptedFootprint>,
    admitted: CellSet,
    consumers: CellSet,
    random_touched: CellSet,
    memo_usage: run::MemoRetentionUsage,
    journal_len: usize,
    admitted_len: usize,
    notifications: Vec<Completion>,
    non_repeatable: bool,
}
/// Actual attempted geometry and its already-proved collision with prior settled ownership.
#[derive(Clone, Copy)]
struct AttemptedFootprint {
    area: Area,
    settled_collision: bool,
}
/// Owned cell completion withheld until actual ownership has settled.
struct Completion {
    consumer: Option<EvaluationTarget>,
    target: EvaluationTarget,
    provenance: CellProvenance,
    result: Result<CellValue, EvaluationFailure>,
}
/// A completed worksheet value and its optional unpublished rectangular footprint.
pub(super) struct ComputedCell {
    pub(super) value: CellValue,
    pub(super) rows: Option<ArrayView>,
}
impl ComputedCell {
    /// Carry a scalar result without a generated footprint.
    pub(super) fn scalar(value: CellValue) -> Self {
        Self { value, rows: None }
    }
}
/// A computed formula withheld from ordinary caches until ownership settles.
struct Held {
    result: ComputedCell,
    source_cached: bool,
    retained: bool,
}
/// Spill roles used by private admissions and settled retained publication.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum SpillRole {
    /// A scalar result without a generated footprint.
    #[default]
    None,
    /// Producer whose complete rectangular footprint ends at this address.
    Anchor { last: CellAddress },
    /// Generated result owned by another source cell on the same sheet.
    Child { anchor: CellAddress },
}
impl SpillRole {
    /// Resolve ownership relative to the address holding this record.
    pub(crate) fn owner(self, address: CellAddress) -> Option<CellAddress> {
        match self {
            Self::None => None,
            Self::Anchor { .. } => Some(address),
            Self::Child { anchor } => Some(anchor),
        }
    }
    /// Return the admitted extent only for a producer.
    pub(crate) fn last(self) -> Option<CellAddress> {
        match self {
            Self::Anchor { last } => Some(last),
            _ => None,
        }
    }
}
impl<'a> Session<'a> {
    /// Resolve one anchor to its finalized footprint without materializing referenced values.
    pub(super) fn spill_reference(&mut self, value: Value) -> EvalResult<Value> {
        let area = match value {
            Value::Refs(areas) | Value::SheetSpan(areas) => match areas.as_slice() {
                [area] if area.size() == 1 => *area,
                _ => return Ok(err(Ref)),
            },
            Value::SignedRefs(areas) => match areas.as_slice() {
                [area] if area.size() == 1 => match area.physical() {
                    Some(area) => area,
                    None => return Ok(err(Ref)),
                },
                _ => return Ok(err(Ref)),
            },
            Value::Scalar(Error(error)) | Value::SelectedScalar(Error(error), _) => {
                return Ok(err(error));
            }
            _ => return Ok(err(Ref)),
        };
        let anchor = CellAddress {
            row: area.r0,
            col: area.c0,
        };
        // Spill references consume extent/ownership, not merely the anchor's
        // scalar value. Scalar dependency equality cannot certify this read.
        if let Some(reads) = &mut self.overlay_reads {
            reads.mark_metadata(self.runtime.consumer);
        }
        let value = self.cell(area.sheet, anchor)?;
        let Some(last) = self
            .runtime
            .preparation
            .roles
            .get(&(area.sheet, anchor))
            .and_then(|role| role.last())
        else {
            return Ok(match value {
                Error(error) => err(error),
                _ => err(Ref),
            });
        };
        self.runtime.budget.allocation(1)?;
        Ok(Value::Refs(ReferenceAreas::One(Area {
            kind: AreaKind::Area,
            sheet: area.sheet,
            r0: anchor.row,
            r1: last.row,
            c0: anchor.col,
            c1: last.col,
        })))
    }

    /// Restore an entire validated footprint together. A cached scalar child alone
    /// cannot establish spill ownership or reserve the rest of its producer's cells.
    pub(super) fn restore_cached_spill(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let Some(cache) = self.overlay_cache else {
            return Ok(());
        };
        let restored = {
            let cache = cache
                .try_borrow()
                .map_err(|_| EvaluationFailure::StalePlan)?;
            let validated = |key| {
                self.overlay_reads
                    .as_ref()
                    .is_some_and(|reads| reads.validated.contains(&key))
            };
            let Some(owner) = cache
                .retained((sheet, at))
                .filter(|entry| entry.validity.is_clean() || validated((sheet, at)))
                .and_then(|c| c.dependencies.spill.owner(at))
            else {
                return Ok(());
            };
            if self.runtime.cache.contains_key(&(sheet, owner)) {
                return Ok(());
            }
            let owner_validated = validated((sheet, owner));
            let Some(last) = cache
                .retained((sheet, owner))
                .filter(|entry| entry.validity.is_clean() || owner_validated)
                .and_then(|c| c.dependencies.spill.last())
            else {
                return Ok(());
            };
            let height = last
                .row
                .checked_sub(owner.row)
                .and_then(|height| height.checked_add(1))
                .ok_or(EvaluationFailure::StalePlan)? as usize;
            let width = last
                .col
                .checked_sub(owner.col)
                .and_then(|width| width.checked_add(1))
                .ok_or(EvaluationFailure::StalePlan)? as usize;
            self.runtime
                .budget
                .check_allocation(height.saturating_mul(width))?;
            let mut rows = Vec::with_capacity(height);
            for row in owner.row..=last.row {
                let mut values = Vec::with_capacity(width);
                for col in owner.col..=last.col {
                    let Some(entry) = cache
                        .retained((sheet, CellAddress { row, col }))
                        .filter(|entry| entry.validity.is_clean() || owner_validated)
                    else {
                        return Ok(());
                    };
                    let address = CellAddress { row, col };
                    if entry.dependencies.spill.owner(address) != Some(owner)
                        || (address == owner && entry.dependencies.spill.last() != Some(last))
                    {
                        return Ok(());
                    }
                    self.runtime.budget.text(entry.value.text_storage_bytes())?;
                    values.push(entry.value.as_ref().clone());
                }
                rows.push(values);
            }
            (owner, rows)
        };
        let value = restored
            .1
            .first()
            .and_then(|row| row.first())
            .ok_or(EvaluationFailure::StalePlan)?
            .clone();
        self.runtime.preparation.held.insert(
            (sheet, restored.0),
            Held {
                result: ComputedCell {
                    value,
                    rows: Some(ArrayView::unknown(restored.1)),
                },
                source_cached: true,
                retained: true,
            },
        );
        if self.runtime.budget.profiling {
            self.runtime.run_usage.external_cache_hits =
                self.runtime.run_usage.external_cache_hits.saturating_add(1);
        }
        Ok(())
    }
    /// Stage a producer's spill after checking blockers and ownership.
    pub(super) fn stage_spill(
        &mut self,
        sheet: usize,
        at: CellAddress,
        rows: ArrayView,
    ) -> EvalResult<ComputedCell> {
        if !self.runtime.visiting.contains(&(sheet, at)) {
            return Err(self.spill_unsupported(sheet,at,"dynamic array publication requires a source formula cell; ad hoc probes are unsupported"));
        }
        let width = rows.first().map_or(0, Vec::len);
        if width == 0 || rows.iter().any(|row| row.len() != width) {
            return Ok(ComputedCell::scalar(Error(VALUE_ERROR)));
        }
        let height = rows.len();
        let last = at
            .row
            .checked_add(u32::try_from(height - 1).unwrap_or(u32::MAX))
            .zip(
                at.col
                    .checked_add(u32::try_from(width - 1).unwrap_or(u32::MAX)),
            )
            .map(|(row, col)| CellAddress { row, col });
        if last.is_none_or(|last| last.validate().is_err()) {
            return Ok(ComputedCell::scalar(Error(CellError::Spill)));
        }
        let count = height
            .checked_mul(width)
            .ok_or_else(|| self.spill_unsupported(sheet, at, "array shape overflow"))?;
        self.runtime.budget.check_allocation(count)?;
        // SAFETY: The preceding last.is_none_or guard returns on None before this unwrap; last is not reassigned.
        let last = last.unwrap();
        self.read_spill_membership(sheet, CellAddress { row: 0, col: 0 }, last);
        let effective = run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs);
        for r in 0..height {
            for c in 0..width {
                self.runtime.budget.charge(WorkKind::ReferencePair, 1)?;
                // SAFETY: The validated last coordinate proves every r < height
                // and c < width fits the grid and both coordinate additions.
                let child = CellAddress {
                    row: at.row + r as u32,
                    col: at.col + c as u32,
                };
                let competing_owner = self
                    .runtime
                    .preparation
                    .roles
                    .get(&(sheet, child))
                    .is_some_and(|role| role.owner(child) != Some(at));
                if !competing_owner
                    && let Some(conflict) = self
                        .runtime
                        .preparation
                        .producer_reads
                        .self_read((sheet, at), (sheet, child))
                {
                    return Err(self.spill_unsupported(
                        sheet,
                        at,
                        if conflict == SelfRead::Sparse {
                            "array result reads its own unpublished sparse footprint"
                        } else {
                            "array result reads its own unpublished footprint"
                        },
                    ));
                }
                if child != at
                    && (effective
                        .cell(sheet, child)
                        .is_some_and(|cell| cell.formula.is_some() || cell.value != Blank))
                {
                    return Ok(ComputedCell::scalar(Error(CellError::Spill)));
                }
            }
        }
        let first = rows
            .first()
            .and_then(|row| row.first())
            .ok_or(EvaluationFailure::StalePlan)?
            .clone();
        Ok(ComputedCell {
            value: first,
            rows: Some(rows),
        })
    }
    /// Attribute an unsupported spill capability to its producer cell.
    pub(super) fn spill_unsupported(
        &self,
        sheet: usize,
        address: CellAddress,
        detail: &str,
    ) -> EvaluationFailure {
        settlement::unsupported(sheet, address, detail)
    }
    /// Admit a complete result through a port with no workbook or execution access.
    pub(super) fn admit_cell(
        &mut self,
        sheet: usize,
        at: CellAddress,
        result: ComputedCell,
    ) -> EvalResult<CellValue> {
        let runtime = &mut *self.runtime;
        admission::Admission {
            budget: &mut runtime.budget,
            cache: &mut runtime.cache,
            preparation: &mut runtime.preparation,
            admitted: &mut runtime.admitted,
            memo_usage: &mut runtime.memo_usage,
            consumer: runtime.consumer,
            limits: self.memo_limits,
            journal: self.overlay_reads.is_some(),
        }
        .admit(sheet, at, result)
    }
    /// Borrow or prepare a source formula using the existing exact-source compiler cache.
    fn source_formula(
        &self,
        sheet: usize,
        at: CellAddress,
    ) -> Option<std::sync::Arc<PreparedFormula>> {
        let source = run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs)
            .cell(sheet, at)?
            .formula
            .as_deref()?;
        self.plan
            .and_then(|plan| plan.formula(EvaluationTarget { sheet, address: at }))
            .filter(|prepared| prepared.source() == source)
            .cloned()
            .or_else(|| Some(self.prepare_source(source)))
    }
    /// Read formal source geometry without evaluating the producer.
    fn source_spill_shape(
        &self,
        sheet: usize,
        at: CellAddress,
        source: &str,
    ) -> Option<poirot_model::ShapeBounds> {
        self.source_preparation()
            .spill_shape(self.plan, sheet, at, source)
    }
    /// Borrow the authoritative index only for this nonrecursive inspection.
    fn with_discovery<R>(
        &self,
        sheet: usize,
        inspect: impl FnOnce(&Discovery) -> R,
    ) -> EvalResult<R> {
        if let Some(shared) = self.spill_candidates {
            let index = shared
                .try_borrow()
                .map_err(|_| EvaluationFailure::StalePlan)?;
            return index
                .get(&sheet)
                .map(inspect)
                .ok_or(EvaluationFailure::StalePlan);
        }
        self.runtime
            .preparation
            .candidates
            .get(&sheet)
            .map(inspect)
            .ok_or(EvaluationFailure::StalePlan)
    }
    /// Discover source candidates without granting formula execution or observer access.
    fn discover_candidates(&mut self, sheet: usize, last: CellAddress) -> EvalResult<()> {
        let source_preparation = self.source_preparation();
        let runtime = &mut *self.runtime;
        discovery::Sources {
            workbook: self.workbook,
            inputs: self.inputs,
            plan: self.plan,
            source_preparation,
            shared: self.spill_candidates,
            local: &mut runtime.preparation.candidates,
            budget: &mut runtime.budget,
            consumer: runtime.consumer,
        }
        .discover(sheet, last)
    }
    #[cfg(test)]
    /// Inspect the full source domain for existing candidate-geometry unit tests.
    fn producer_candidates(&mut self, sheet: usize) -> EvalResult<Vec<Candidate>> {
        self.discover_candidates(
            sheet,
            CellAddress {
                row: poirot_model::MAX_ROWS - 1,
                col: poirot_model::MAX_COLUMNS - 1,
            },
        )?;
        self.with_discovery(sheet, |p| p.candidates.clone())
    }
    /// Discard this attempt's admissions and rewind only newly entered cell streams.
    fn rollback_spills(&mut self) {
        let runtime = &mut *self.runtime;
        let Some(active) = runtime.preparation.settlement.active_mut() else {
            return;
        };
        for key in active.admitted.drain() {
            runtime.cache.remove(&key);
            runtime.preparation.roles.remove(&key);
        }
        for key in active.random_touched.drain() {
            runtime.random_streams.remove(&key);
        }
        runtime.memo_usage = active.memo_usage;
        runtime.preparation.journal.truncate(active.journal_len);
        if let Some(admitted) = &mut runtime.admitted {
            admitted.truncate(active.admitted_len);
        }
        if let Some(reads) = &mut self.overlay_reads {
            reads.discard_consumers(&active.consumers);
        }
        runtime.preparation.held.clear();
        active.consumers.clear();
        active.footprints.clear();
        active.notifications.clear();
        runtime.preparation.producer_reads.clear();
    }
    /// Actual external contracts, rather than conservative static effects, govern safe retries.
    pub(super) fn record_spill_effect(&mut self, read: &DependencyRead) {
        let runtime = &mut *self.runtime;
        let non_repeatable = match read {
            DependencyRead::OpaqueProvider(_) => true,
            DependencyRead::Extension(invocation) => matches!(
                invocation.binding.registration.contract.cacheability,
                extensions::ExtensionCacheability::Volatile
                    | extensions::ExtensionCacheability::Opaque
            ),
            _ => false,
        };
        if let Some(active) = runtime.preparation.settlement.active_mut() {
            if non_repeatable {
                active.non_repeatable = true;
            }
            if let Some(consumer) = runtime.consumer {
                active.consumers.insert((consumer.sheet, consumer.address));
            }
            if let DependencyRead::Random(random) = read
                && let Ok(sheet) = usize::try_from(random.sheet)
            {
                active.random_touched.insert((
                    sheet,
                    CellAddress {
                        row: random.row,
                        col: random.column,
                    },
                ));
            }
        }
    }
    /// Deliver completion through the preparation state without exposing other session services.
    pub(super) fn complete_cell(&mut self, completion: CellCompletion<'_>) -> EvalResult<()> {
        let runtime = &mut *self.runtime;
        runtime.preparation.complete_cell(
            &mut runtime.budget,
            runtime.consumer,
            &mut self.cell_observer,
            completion,
        )
    }
    /// Restore caller-trusted source values without evaluating the producer expression.
    fn restore_saved_producer(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let worksheet = self
            .workbook
            .sheet(sheet)
            .ok_or(EvaluationFailure::StalePlan)?;
        let effective = run_overlay::EffectiveWorkbook::new(self.workbook, self.inputs);
        let cell = effective
            .cell(sheet, at)
            .ok_or(EvaluationFailure::StalePlan)?;
        let baseline = worksheet.source_cell(at);
        let unchanged_anchor = baseline.is_some_and(|base| {
            base.formula == cell.formula && same_result_identity(&base.value, &cell.value)
        }) && !self.inputs.is_some_and(|inputs| {
            matches!(
                inputs.edit(sheet, at),
                Some(run_overlay::CellEdit::Replace(_))
            )
        });
        self.runtime
            .budget
            .check_text(cell.value.text_storage_bytes())
            .map_err(|mut failure| {
                failure.set_target(EvaluationTarget { sheet, address: at });
                failure
            })?;
        let value = cell.value.clone();
        let mut rows = None;
        if unchanged_anchor {
            let declared = worksheet.spill_extent(at);
            let mut owned = false;
            if declared.is_none() {
                for (_, owner) in worksheet.spill_children() {
                    self.runtime.budget.charge(WorkKind::SparseScan, 1)?;
                    if owner == at {
                        owned = true;
                        break;
                    }
                }
            }
            if declared.is_some() || owned {
                self.runtime.budget.charge(
                    WorkKind::SparseScan,
                    worksheet.spill_children().count() as u64,
                )?;
                let last = worksheet.complete_spill_extent(at).ok_or_else(|| {
                    self.spill_unsupported(
                        sheet,
                        at,
                        "caller-trusted saved spill is incomplete or lacks declared geometry",
                    )
                })?;
                let height = last
                    .row
                    .checked_sub(at.row)
                    .and_then(|n| n.checked_add(1))
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or(EvaluationFailure::StalePlan)?;
                let width = last
                    .col
                    .checked_sub(at.col)
                    .and_then(|n| n.checked_add(1))
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or(EvaluationFailure::StalePlan)?;
                self.runtime
                    .budget
                    .check_allocation(height.saturating_mul(width))?;
                let mut saved = Vec::new();
                saved
                    .try_reserve_exact(height)
                    .map_err(|_| EvaluationFailure::ResourceLimit {
                        capability: CapabilityId::FunctionAdapter,
                        source: Some(EvaluationTarget { sheet, address: at }),
                        detail: "saved spill row allocation failed".into(),
                    })?;
                for row in at.row..=last.row {
                    let mut values = Vec::new();
                    values.try_reserve_exact(width).map_err(|_| {
                        EvaluationFailure::ResourceLimit {
                            capability: CapabilityId::FunctionAdapter,
                            source: Some(EvaluationTarget { sheet, address: at }),
                            detail: "saved spill value allocation failed".into(),
                        }
                    })?;
                    for col in at.col..=last.col {
                        let address = CellAddress { row, col };
                        self.runtime.budget.charge(WorkKind::SparseScan, 1)?;
                        let source = worksheet.source_cell(address);
                        let effective = effective.cell(sheet, address);
                        let explicit_content = self.inputs.is_some_and(|inputs| {
                            matches!(
                                inputs.edit(sheet, address),
                                Some(run_overlay::CellEdit::Replace(_))
                            )
                        });
                        if address != at
                            && (explicit_content
                                || !match (source, effective) {
                                    (None, None) => true,
                                    (Some(base), Some(current)) => {
                                        base.formula == current.formula
                                            && same_result_identity(&base.value, &current.value)
                                    }
                                    // A style-only override can materialize an absent blank source.
                                    (None, Some(current)) => {
                                        current.formula.is_none() && current.value == Blank
                                    }
                                    _ => false,
                                })
                        {
                            return Err(self.spill_unsupported(
                                sheet,
                                at,
                                "caller-trusted saved spill has an edited child",
                            ));
                        }
                        let cached = worksheet.get(address).ok_or(EvaluationFailure::StalePlan)?;
                        self.runtime
                            .budget
                            .text(cached.value.text_storage_bytes())?;
                        // SAFETY: width slots were reserved before this exactly-width row loop.
                        values.push(cached.value.clone());
                    }
                    // SAFETY: height slots were reserved before this exactly-height row loop.
                    saved.push(values);
                }
                rows = Some(ArrayView::unknown(saved));
            }
        }
        let key = (sheet, at);
        let previous = self
            .runtime
            .consumer
            .replace(EvaluationTarget { sheet, address: at });
        self.runtime.visiting.insert(key);
        let staged = if let Some(saved) = rows {
            self.stage_spill(sheet, at, saved)
        } else {
            Ok(ComputedCell::scalar(value))
        };
        self.runtime.visiting.remove(&key);
        self.runtime.consumer = previous;
        let result = staged?;
        self.runtime.preparation.held.insert(
            key,
            Held {
                result,
                source_cached: true,
                retained: false,
            },
        );
        Ok(())
    }
    /// Compute a private raw payload; dependency reads resolve within the current attempt.
    fn compute_producer(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let key = (sheet, at);
        if self.runtime.cache.contains_key(&key) || self.runtime.preparation.held.contains_key(&key)
        {
            return Ok(());
        }
        if self.runtime.preparation.computing.contains(&key) {
            return Err(self.spill_unsupported(
                sheet,
                at,
                "circular unpublished producer dependency",
            ));
        }
        if self.cell_reader.is_some() {
            return Err(self.spill_unsupported(
                sheet,
                at,
                "dynamic spill ownership is incompatible with an external scalar cell cache",
            ));
        }
        let conditional = self
            .overlay_cache
            .and_then(|cache| {
                cache.try_borrow().ok().and_then(|cache| {
                    cache.retained(key).map(|entry| {
                        matches!(
                            entry.validity,
                            overlay::dependencies::Validity::Cause { .. }
                                | overlay::dependencies::Validity::Causes(_)
                        )
                    })
                })
            })
            .unwrap_or(false);
        if conditional {
            // Dirty evidence can itself read a potential child of this producer.
            // Guard validation just like execution against unpublished recursion.
            self.runtime.preparation.computing.insert(key);
            let validated = self.revalidate_cached(key);
            self.runtime.preparation.computing.remove(&key);
            if validated?.is_some()
                && let Some(reads) = &mut self.overlay_reads
            {
                reads.fresh_results.insert(key);
            }
        }
        self.restore_cached_spill(sheet, at)?;
        if self.runtime.preparation.held.contains_key(&key) {
            return Ok(());
        }
        let retained = self.overlay_cache.and_then(|cache| {
            cache
                .try_borrow()
                .ok()?
                .retained(key)
                .filter(|entry| {
                    entry.validity.is_clean()
                        || self
                            .overlay_reads
                            .as_ref()
                            .is_some_and(|reads| reads.validated.contains(&key))
                })
                .filter(|entry| entry.dependencies.spill.owner(at).is_none())
                .map(|entry| entry.value.as_ref().clone())
        });
        if let Some(value) = retained {
            self.runtime.preparation.held.insert(
                key,
                Held {
                    result: ComputedCell::scalar(value),
                    source_cached: true,
                    retained: true,
                },
            );
            if self.runtime.budget.profiling {
                self.runtime.run_usage.external_cache_hits =
                    self.runtime.run_usage.external_cache_hits.saturating_add(1);
            }
            if let Some(reads) = &mut self.overlay_reads {
                reads.completed.insert(key);
            }
            return Ok(());
        }
        if self.producer_policy == ProducerPolicy::TrustSaved {
            let result = self.restore_saved_producer(sheet, at);
            if let Err(failure) = &result {
                let _ = self.complete_cell(CellCompletion {
                    consumer: self.runtime.consumer,
                    target: EvaluationTarget { sheet, address: at },
                    provenance: CellProvenance::ExternalCache,
                    result: Err(failure),
                });
            }
            return result;
        }
        let prepared = self
            .source_formula(sheet, at)
            .ok_or(EvaluationFailure::StalePlan)?;
        let target = EvaluationTarget { sheet, address: at };
        self.runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(target);
                failure
            })?;
        self.runtime.budget.enter_depth()?;
        self.runtime.budget.push_cell(target);
        self.runtime.preparation.computing.insert(key);
        self.runtime.visiting.insert(key);
        let previous = self.runtime.consumer.replace(target);
        let demand = self.root_demand();
        let previous_demand = std::mem::replace(&mut self.expression_demand, demand);
        if self.runtime.budget.profiling {
            self.runtime.run_usage.formula_executions =
                self.runtime.run_usage.formula_executions.saturating_add(1);
        }
        let result = (|| {
            self.runtime.budget.text(prepared.source().len())?;
            let value = self.prepared_expression(sheet, at, &prepared)?;
            self.publish_worksheet_result(sheet, at, value)
        })();
        self.expression_demand = previous_demand;
        self.runtime.consumer = previous;
        self.runtime.visiting.remove(&key);
        self.runtime.preparation.computing.remove(&key);
        self.runtime.budget.pop_cell();
        self.runtime.budget.leave_depth();
        self.runtime.preparation.producer_reads.forget(key);
        if let Err(failure) = &result {
            // An operational failure is final; unlike a held successful payload,
            // it must retain the normal failed-cell completion notification.
            let _ = self.complete_cell(CellCompletion {
                consumer: previous,
                target,
                provenance: CellProvenance::Evaluated,
                result: Err(failure),
            });
        }
        let result = result?;
        self.runtime.preparation.held.insert(
            key,
            Held {
                result,
                source_cached: false,
                retained: false,
            },
        );
        Ok(())
    }
    /// Admit one private attempt while preserving raw geometry for ownership arbitration.
    fn finalize_producer(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        let Some(mut held) = self.runtime.preparation.held.remove(&(sheet, at)) else {
            return Ok(());
        };
        if let Some(rows) = &held.result.rows {
            let last_row = u32::try_from(rows.len().saturating_sub(1))
                .ok()
                .and_then(|n| at.row.checked_add(n))
                .ok_or(EvaluationFailure::StalePlan)?;
            let last_col = rows
                .first()
                .and_then(|r| u32::try_from(r.len().saturating_sub(1)).ok())
                .and_then(|n| at.col.checked_add(n))
                .ok_or(EvaluationFailure::StalePlan)?;
            let runtime = &mut *self.runtime;
            if let Some(active) = runtime.preparation.settlement.active_mut() {
                let settled_collision = (at.row..=last_row).any(|row| {
                    (at.col..=last_col).any(|col| {
                        let address = CellAddress { row, col };
                        runtime
                            .preparation
                            .roles
                            .get(&(sheet, address))
                            .is_some_and(|role| role.owner(address) != Some(at))
                            && !active.admitted.contains(&(sheet, address))
                    })
                });
                active.footprints.insert(
                    (sheet, at),
                    AttemptedFootprint {
                        area: Area {
                            kind: AreaKind::Area,
                            sheet,
                            r0: at.row,
                            c0: at.col,
                            r1: last_row,
                            c1: last_col,
                        },
                        settled_collision,
                    },
                );
                if active.blocked.contains(&(sheet, at)) || settled_collision {
                    held.result.rows = None;
                    held.result.value = Error(CellError::Spill);
                }
            }
        }
        let admission = self.admit_cell(sheet, at, held.result);
        if let Err(failure) = &admission {
            let _ = self.complete_cell(CellCompletion {
                consumer: self.runtime.consumer,
                target: EvaluationTarget { sheet, address: at },
                provenance: if held.source_cached {
                    CellProvenance::ExternalCache
                } else {
                    CellProvenance::Evaluated
                },
                result: Err(failure),
            });
        }
        let value = admission?;
        let last = self
            .runtime
            .preparation
            .roles
            .get(&(sheet, at))
            .and_then(|role| match role {
                SpillRole::Anchor { last } => Some(*last),
                SpillRole::Child { .. } | SpillRole::None => None,
            })
            .unwrap_or(at);
        // New geometry can enter a previously absent indexed gap even when the
        // producer anchor lies outside the search axis. Valid retained units use
        // restore_cached_spill instead and preserve their existing prefix indexes.
        if !held.retained {
            self.runtime
                .lookup_prefixes
                .invalidate_footprint(sheet, at, last);
        }
        {
            self.complete_cell(CellCompletion {
                consumer: self.runtime.consumer,
                target: EvaluationTarget { sheet, address: at },
                provenance: if held.source_cached {
                    CellProvenance::ExternalCache
                } else {
                    CellProvenance::Evaluated
                },
                result: Ok(&value),
            })?;
        }
        if let Some(reads) = &mut self.overlay_reads {
            reads.completed.insert((sheet, at));
            if !held.retained {
                reads.fresh_results.insert((sheet, at));
            }
        }
        Ok(())
    }
    /// Whether all possible owners of this physical area have settled.
    pub(super) fn ready_area(&self, area: Area) -> bool {
        self.runtime.preparation.failure.is_none()
            && self.runtime.preparation.settlement.active().is_none()
            && (self
                .runtime
                .preparation
                .ready
                .is_some_and(|SettledArea(ready)| {
                    ready.sheet == area.sheet
                        && ready.r0 <= area.r0
                        && area.r1 <= ready.r1
                        && ready.c0 <= area.c0
                        && area.c1 <= ready.c1
                })
                || self
                    .with_discovery(area.sheet, |discovery| {
                        discovery.covers(CellAddress {
                            row: area.r1,
                            col: area.c1,
                        }) && discovery.possible_in(area, None).iter().all(|c| {
                            !c.intersects(area)
                                || self
                                    .runtime
                                    .preparation
                                    .settled
                                    .contains(&(area.sheet, c.anchor))
                        })
                    })
                    .unwrap_or(false))
    }
    /// Whether reads currently participate in private ownership settlement.
    pub(super) fn settling_spills(&self) -> bool {
        self.runtime.preparation.settlement.active().is_some()
    }
    /// Read validated candidate membership without another compiler lookup.
    pub(super) fn spill_candidate(&self, sheet: usize, at: CellAddress) -> bool {
        self.with_discovery(sheet, |p| {
            p.candidates.binary_search_by_key(&at, |c| c.anchor).is_ok()
        })
        .unwrap_or(false)
    }
    /// Resolve a demanded area and retain a future-growth membership guard.
    pub(super) fn prepare_spills_in(&mut self, area: Area) -> EvalResult<()> {
        self.resolve_spills_in(area)?;
        if self.array_result_policy == ArrayResultPolicy::DynamicSpill {
            self.read_spill_membership(
                area.sheet,
                CellAddress { row: 0, col: 0 },
                CellAddress {
                    row: area.r1,
                    col: area.c1,
                },
            );
        }
        Ok(())
    }
    /// Settle a point read using its already resolved immutable source and ownership eligibility.
    pub(super) fn resolve_cell_spills(
        &mut self,
        sheet: usize,
        address: CellAddress,
        source: session_execution::ResolvedSource<'a>,
    ) -> EvalResult<()> {
        self.settle_spills_in(
            Area {
                kind: AreaKind::Cell,
                sheet,
                r0: address.row,
                r1: address.row,
                c0: address.col,
                c1: address.col,
            },
            Some(source),
        )
    }
    /// Complete a range's preparation before retaining its bounded member-read proof.
    pub(super) fn resolve_spills_in(&mut self, area: Area) -> EvalResult<()> {
        self.settle_spills_in(area, None)?;
        // Point reads preserve the enclosing operand. Nested private attempts may
        // complete provisional work, but cannot publish proof across rollback.
        if area.kind == AreaKind::Area
            && self.array_result_policy == ArrayResultPolicy::DynamicSpill
            && self.runtime.preparation.settlement.active().is_none()
        {
            self.runtime.preparation.ready = Some(SettledArea(area));
        }
        Ok(())
    }
    /// Attach the recursive evaluator adapter to the spill settlement engine.
    fn settle_spills_in(
        &mut self,
        area: Area,
        source: Option<session_execution::ResolvedSource<'a>>,
    ) -> EvalResult<()> {
        settlement::Engine::new(self).settle(area, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn held_raw_results_have_no_ordinary_completion_or_ownership() {
        let mut book = Workbook::new();
        let anchor = CellAddress { row: 0, col: 0 };
        book.add_sheet("S")
            .unwrap()
            .set_formula(anchor, "{1,2;3,4}")
            .unwrap();
        let mut session = Session::with_context(&book, Default::default());
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        session.overlay_reads = Some(Default::default());
        session.runtime.admitted = Some(Vec::new());
        session.compute_producer(0, anchor).unwrap();
        assert!(session.runtime.preparation.held.contains_key(&(0, anchor)));
        assert!(session.runtime.preparation.publication().is_err());
        assert!(session.runtime.cache.is_empty());
        assert!(session.runtime.preparation.roles.is_empty());
        assert!(session.runtime.preparation.journal.is_empty());
        assert!(session.runtime.admitted.as_ref().unwrap().is_empty());
        assert!(session.overlay_reads.as_ref().unwrap().completed.is_empty());
        assert!(
            session
                .overlay_reads
                .as_ref()
                .unwrap()
                .fresh_results
                .is_empty()
        );
        assert_eq!(session.runtime.budget.usage.completed_cells, 0);
        session.finalize_producer(0, anchor).unwrap();
        assert_eq!(session.runtime.cache.len(), 4);
        assert_eq!(session.runtime.preparation.journal.len(), 4);
        assert_eq!(session.runtime.budget.usage.completed_cells, 4);
    }
}

#[cfg(test)]
mod candidate_tests {
    use super::*;
    use poirot_model::ShapeBounds as S;
    #[test]
    fn bounded_candidates_match_an_independent_coordinate_membership_oracle() {
        for sheet in 0..2 {
            for row in [0, 2, poirot_model::MAX_ROWS - 1] {
                for col in [0, 3, poirot_model::MAX_COLUMNS - 1] {
                    let anchor = CellAddress { row, col };
                    for shape in [S::new(2, 4), S::new(0, 1), S::new(1, 0), S::UNKNOWN] {
                        let candidate = Candidate::new(sheet, anchor, shape);
                        for target_sheet in 0..2 {
                            for r0 in [0, 1, 2, 4, poirot_model::MAX_ROWS - 1] {
                                for c0 in [0, 1, 3, 5, poirot_model::MAX_COLUMNS - 1] {
                                    let demand = Area {
                                        kind: AreaKind::Area,
                                        sheet: target_sheet,
                                        r0,
                                        r1: r0.saturating_add(1).min(poirot_model::MAX_ROWS - 1),
                                        c0,
                                        c1: c0.saturating_add(1).min(poirot_model::MAX_COLUMNS - 1),
                                    };
                                    let row_inside = |r| {
                                        r >= row
                                            && shape.rows.is_none_or(|n| {
                                                u64::from(r) - u64::from(row) < u64::from(n.get())
                                            })
                                    };
                                    let col_inside = |c| {
                                        c >= col
                                            && shape.columns.is_none_or(|n| {
                                                u64::from(c) - u64::from(col) < u64::from(n.get())
                                            })
                                    };
                                    let oracle = sheet == target_sheet
                                        && (demand.r0..=demand.r1).any(row_inside)
                                        && (demand.c0..=demand.c1).any(col_inside);
                                    assert_eq!(candidate.intersects(demand), oracle);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn source_owned_index_omits_scalar_and_preserves_static_disjointness() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for (address, formula) in [
            ("A1", "SEQUENCE(3)"),
            ("C1", "SEQUENCE(A10,2)"),
            ("F1", "SUM(SEQUENCE(4))"),
            ("H1", "bad_name"),
            ("J1", "{1,2}"),
        ] {
            sheet
                .set_formula(address.parse().unwrap(), formula)
                .unwrap();
        }
        book.add_sheet("Other").unwrap();
        let mut session = Session::with_context(&book, EvaluationInputs::default());
        let candidates = session.producer_candidates(0).unwrap();
        assert_eq!(candidates.len(), 4);
        for (address, winners) in [
            ("A3", vec!["A1"]),
            ("A4", vec![]),
            ("D8", vec!["C1"]),
            ("E8", vec![]),
            ("I1", vec!["H1"]),
            ("K1", vec!["H1", "J1"]),
        ] {
            let at: CellAddress = address.parse().unwrap();
            let demand = Area {
                kind: AreaKind::Cell,
                sheet: 0,
                r0: at.row,
                r1: at.row,
                c0: at.col,
                c1: at.col,
            };
            let actual: Vec<_> = candidates
                .iter()
                .filter(|c| c.intersects(demand))
                .map(|c| c.anchor)
                .collect();
            let expected: Vec<CellAddress> = winners.iter().map(|s| s.parse().unwrap()).collect();
            assert_eq!(actual, expected, "{address}");
        }
        assert!(session.producer_candidates(1).unwrap().is_empty());
    }
    #[test]
    fn saved_extents_widen_the_candidate_without_trusting_computed_dimensions() {
        for declared in [false, true] {
            let mut book = Workbook::new();
            let sheet = book.add_sheet("S").unwrap();
            let anchor: CellAddress = "A1".parse().unwrap();
            sheet.set_formula(anchor, "SEQUENCE(1)").unwrap();
            sheet
                .set_spill_cached_value("D4".parse().unwrap(), anchor, "SEQUENCE(1)", 7.0.into())
                .unwrap();
            if declared {
                sheet
                    .set_spill_extent(anchor, "D4".parse().unwrap(), "SEQUENCE(1)")
                    .unwrap();
            }
            let mut session = Session::with_context(&book, EvaluationInputs::default());
            let candidates = session.producer_candidates(0).unwrap();
            let candidate = candidates.first().unwrap();
            assert!(candidate.intersects(Area {
                kind: AreaKind::Cell,
                sheet: 0,
                r0: 3,
                r1: 3,
                c0: 3,
                c1: 3
            }));
            if declared {
                assert_eq!((candidate.area.r1, candidate.area.c1), (3, 3));
            } else {
                assert_eq!(
                    (candidate.area.r1, candidate.area.c1),
                    (poirot_model::MAX_ROWS - 1, poirot_model::MAX_COLUMNS - 1)
                );
            }
        }
    }
}

#[cfg(test)]
mod candidate_failure_tests {
    use super::*;
    #[test]
    fn parser_resource_refusal_cannot_supply_a_narrow_or_absent_candidate() {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula("A1".parse().unwrap(), "SEQUENCE(3,2)")
            .unwrap();
        let policy = EvaluationPolicy {
            parse: poirot_parsing::formula::FormulaParsePolicy {
                max_expressions: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut session = Session::with_context(&book, EvaluationInputs::default());
        session.preparation_policy = Some(&policy);
        let candidates = session.producer_candidates(0).unwrap();
        assert_eq!(candidates.len(), 1);
        let candidate = candidates.first().unwrap();
        assert_eq!(
            (candidate.area.r1, candidate.area.c1),
            (poirot_model::MAX_ROWS - 1, poirot_model::MAX_COLUMNS - 1)
        );
    }
    #[test]
    fn syntax_failure_has_no_successful_spill_and_registry_failure_is_conservative() {
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula("A1".parse().unwrap(), "SEQUENCE(")
            .unwrap();
        let mut session = Session::with_context(&book, EvaluationInputs::default());
        assert!(session.producer_candidates(0).unwrap().is_empty());
        let mut book = Workbook::new();
        book.add_sheet("S")
            .unwrap()
            .set_formula("A1".parse().unwrap(), "SEQUENCE(3)")
            .unwrap();
        let registry = crate::extensions::ExtensionRegistry::new();
        let policy = EvaluationPolicy::with_extensions(Default::default(), &registry);
        let mut session = Session::with_context(&book, EvaluationInputs::default());
        session.preparation_policy = Some(&policy);
        let candidates = session.producer_candidates(0).unwrap();
        assert_eq!(candidates.len(), 1);
        let candidate = candidates.first().unwrap();
        assert_eq!(
            (candidate.area.r1, candidate.area.c1),
            (poirot_model::MAX_ROWS - 1, poirot_model::MAX_COLUMNS - 1)
        );
    }
}

#[cfg(test)]
mod discovery_tests {
    #[test]
    fn completed_component_reuses_maps_without_exposing_previous_attempt_state() {
        let mut slot = SettlementSlot::default();
        assert!(slot.active().is_none());
        slot.begin(Default::default(), 3, 5);
        let state = slot.active_mut().unwrap();
        let key = (0, CellAddress { row: 1, col: 2 });
        state.selected.insert(key);
        state.blocked.insert(key);
        state.admitted.insert(key);
        state.consumers.insert(key);
        state.random_touched.insert(key);
        state.non_repeatable = true;
        let pointer = state as *const Settlement;
        let capacity = state.admitted.capacity();
        slot.finish();
        assert!(slot.active().is_none());
        slot.begin(Default::default(), 7, 11);
        let state = slot.active().unwrap();
        assert_eq!(state as *const Settlement, pointer);
        assert_eq!(state.admitted.capacity(), capacity);
        assert!(state.selected.is_empty());
        assert!(state.blocked.is_empty());
        assert!(state.admitted.is_empty());
        assert!(state.consumers.is_empty());
        assert!(state.random_touched.is_empty());
        assert!(!state.non_repeatable);
        assert_eq!((state.journal_len, state.admitted_len), (7, 11));
    }
    #[test]
    fn completed_range_proofs_are_sheet_specific_and_disabled_after_later_failure() {
        let mut book = Workbook::new();
        book.add_sheet("S").unwrap();
        book.add_sheet("T")
            .unwrap()
            .set_formula("Z1".parse().unwrap(), "SEQUENCE(100)")
            .unwrap();
        let mut session = Session::with_context(&book, Default::default());
        let area = Area {
            kind: AreaKind::Area,
            sheet: 0,
            r0: 0,
            r1: 1,
            c0: 0,
            c1: 0,
        };
        session.resolve_spills_in(area).unwrap();
        assert!(session.ready_area(area));
        assert!(!session.ready_area(Area { sheet: 1, ..area }));
        session.runtime.budget = BudgetMeter::new(EvaluationBudgets {
            materialized_values: 2,
            ..Default::default()
        })
        .unwrap();
        assert!(
            session
                .resolve_spills_in(Area {
                    sheet: 1,
                    r0: 9,
                    r1: 9,
                    c0: 25,
                    c1: 25,
                    ..area
                })
                .is_err()
        );
        assert!(session.runtime.preparation.failure.is_some());
        assert!(!session.ready_area(area));
        assert!(session.resolve_spills_in(area).is_err());
        assert!(session.runtime.preparation.settlement.active().is_none());
    }
    #[test]
    fn private_nested_ranges_never_publish_completed_area_proofs() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet
            .set_formula("A1".parse().unwrap(), "SEQUENCE(SUM(C1:C2))")
            .unwrap();
        sheet.set_value("C1".parse().unwrap(), 1.).unwrap();
        sheet.set_value("C2".parse().unwrap(), 1.).unwrap();
        let mut session = Session::with_context(&book, Default::default());
        let area = Area {
            kind: AreaKind::Cell,
            sheet: 0,
            r0: 1,
            r1: 1,
            c0: 0,
            c1: 0,
        };
        session.resolve_spills_in(area).unwrap();
        assert!(session.runtime.preparation.ready.is_none());
        session
            .prepare_spills_in(Area {
                kind: AreaKind::Area,
                ..area
            })
            .unwrap();
        assert!(session.runtime.preparation.ready.is_some());
        assert!(session.ready_area(area));
        session.runtime.preparation.failure = Some(EvaluationFailure::StalePlan);
        assert!(!session.ready_area(area));
        assert!(session.resolve_spills_in(area).is_err());
    }
    #[test]
    fn candidate_row_filter_matches_an_independent_rectangle_oracle() {
        let mut discovery = Discovery::default();
        // Nested and unbounded-height members deliberately prevent perfect pruning.
        for (row, column, height, width) in [
            (0, 2, Some(1), Some(2)),
            (1, 0, Some(4), Some(1)),
            (2, 4, None, Some(1)),
            (4, 1, Some(1), None),
            (6, 3, Some(2), Some(2)),
        ] {
            let candidate = Candidate::new(
                0,
                CellAddress { row, col: column },
                poirot_model::ShapeBounds {
                    rows: height.and_then(std::num::NonZeroU32::new),
                    columns: width.and_then(std::num::NonZeroU32::new),
                },
            );
            discovery.ending_rows.push(
                discovery
                    .ending_rows
                    .last()
                    .copied()
                    .unwrap_or(0)
                    .max(candidate.area.r1),
            );
            discovery.candidates.push(candidate);
        }
        for r0 in 0..10 {
            for r1 in r0..10 {
                for c0 in 0..7 {
                    for c1 in c0..7 {
                        let area = Area {
                            kind: AreaKind::Area,
                            sheet: 0,
                            r0,
                            r1,
                            c0,
                            c1,
                        };
                        for before in [
                            None,
                            Some(CellAddress { row: 4, col: 1 }),
                            Some(CellAddress { row: 0, col: 0 }),
                        ] {
                            let expected: Vec<_> = discovery
                                .candidates
                                .iter()
                                .filter(|c| {
                                    before.is_none_or(|anchor| c.anchor < anchor)
                                        && c.anchor.row <= r1
                                        && c.area.r1 >= r0
                                        && c.anchor.col <= c1
                                        && c.area.c1 >= c0
                                })
                                .map(|c| c.anchor)
                                .collect();
                            let actual: Vec<_> = discovery
                                .possible_in(area, before)
                                .iter()
                                .filter(|c| c.intersects(area))
                                .map(|c| c.anchor)
                                .collect();
                            assert_eq!(actual, expected);
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn interleaved_discovery_rebuilds_the_candidate_row_summary() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet
            .set_formula(CellAddress { row: 8, col: 0 }, "SEQUENCE(2)")
            .unwrap();
        sheet
            .set_formula(CellAddress { row: 2, col: 4 }, "SEQUENCE(3)")
            .unwrap();
        let mut session = Session::with_context(&book, Default::default());
        session
            .discover_candidates(0, CellAddress { row: 9, col: 0 })
            .unwrap();
        session
            .discover_candidates(0, CellAddress { row: 4, col: 4 })
            .unwrap();
        session
            .with_discovery(0, |p| {
                assert_eq!(
                    p.candidates
                        .iter()
                        .map(|c| c.anchor.row)
                        .collect::<Vec<_>>(),
                    [2, 8]
                );
                assert_eq!(p.ending_rows, [4, 9]);
            })
            .unwrap();
    }
    #[test]
    fn frontier_edges_equality_and_retained_boundary_have_exact_strips() {
        let mut discovery = Discovery::default();
        let mut budget = BudgetMeter::new(Default::default()).unwrap();
        let mut apply = |last: CellAddress| {
            let expansion = discovery
                .expansion(0, last, &mut budget, None)
                .unwrap()
                .unwrap();
            let strips = expansion.strips;
            for col in expansion.removed_columns {
                discovery.inspected.remove(&col);
            }
            discovery.inspected.insert(last.col, last.row);
            strips
        };
        let right = poirot_model::MAX_COLUMNS - 1;
        let bottom = poirot_model::MAX_ROWS - 1;
        let first = apply(CellAddress { row: 0, col: right });
        assert_eq!(first.len(), 1);
        let second = apply(CellAddress {
            row: bottom,
            col: 0,
        });
        assert_eq!(second.len(), 1);
        assert_eq!(
            (
                second[0].first.row,
                second[0].first.col,
                second[0].last.row,
                second[0].last.col
            ),
            (1, 0, bottom, 0)
        );
        let third = apply(CellAddress {
            row: bottom,
            col: right,
        });
        assert_eq!(third.len(), 1);
        assert_eq!(
            (
                third[0].first.row,
                third[0].first.col,
                third[0].last.row,
                third[0].last.col
            ),
            (1, 1, bottom, right)
        );
        assert_eq!(discovery.inspected.len(), 1);
        assert!(
            discovery
                .expansion(
                    0,
                    CellAddress {
                        row: bottom,
                        col: right
                    },
                    &mut budget,
                    None
                )
                .unwrap()
                .is_none()
        );
        discovery.inspected = std::collections::BTreeMap::from([(2, 10), (5, 3)]);
        let expansion = discovery
            .expansion(0, CellAddress { row: 6, col: 4 }, &mut budget, None)
            .unwrap()
            .unwrap();
        assert!(expansion.removed_columns.is_empty());
        assert_eq!(expansion.strips.len(), 1);
        let strip = expansion.strips[0];
        assert_eq!(
            (
                strip.first.row,
                strip.first.col,
                strip.last.row,
                strip.last.col
            ),
            (4, 3, 6, 4)
        );
        let equal = discovery
            .expansion(0, CellAddress { row: 4, col: 5 }, &mut budget, None)
            .unwrap()
            .unwrap();
        assert!(equal.removed_columns.is_empty());
        assert_eq!(equal.frontier_size, 2);
    }
    #[test]
    fn long_antidiagonal_frontier_collapses_without_revisiting_source_strips() {
        let mut discovery = Discovery::default();
        let mut budget = BudgetMeter::new(Default::default()).unwrap();
        for row in 0..1024 {
            let last = CellAddress {
                row,
                col: 1023 - row,
            };
            let expansion = discovery
                .expansion(0, last, &mut budget, None)
                .unwrap()
                .unwrap();
            assert!(expansion.removed_columns.is_empty());
            assert_eq!(expansion.strips.len(), 1);
            assert_eq!(expansion.strips[0].first.row, row);
            discovery.inspected.insert(last.col, last.row);
        }
        let expansion = discovery
            .expansion(
                0,
                CellAddress {
                    row: 1023,
                    col: 1023,
                },
                &mut budget,
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(expansion.removed_columns.len(), 1023);
        assert!(!expansion.removed_columns.contains(&1023));
        assert_eq!(expansion.frontier_size, 1);
        for col in expansion.removed_columns {
            discovery.inspected.remove(&col);
        }
        discovery.inspected.insert(1023, 1023);
        assert_eq!(discovery.inspected.len(), 1);
        assert!(discovery.covers(CellAddress {
            row: 1023,
            col: 1023
        }));
    }

    use super::*;
    #[test]
    fn derived_discovery_preserves_send_and_sync_storage_bounds() {
        fn assert_bounds<T: Send + Sync>() {}
        assert_bounds::<CandidateIndex>();
    }
    #[test]
    fn legacy_owner_storage_counts_distinct_owners_rather_than_children() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        let anchor = CellAddress { row: 0, col: 0 };
        sheet.set_formula(anchor, "SEQUENCE(1)").unwrap();
        for row in 1..256 {
            sheet
                .set_spill_cached_value(
                    CellAddress { row, col: 0 },
                    anchor,
                    "SEQUENCE(1)",
                    1.0.into(),
                )
                .unwrap();
        }
        let mut session = Session::with_context(&book, Default::default());
        session.runtime.budget = BudgetMeter::new(EvaluationBudgets {
            materialized_values: 1,
            ..Default::default()
        })
        .unwrap();
        session.discover_candidates(0, anchor).unwrap();
        let discovery = session.runtime.preparation.candidates.get(&0).unwrap();
        assert_eq!(discovery.legacy.as_ref(), &[anchor]);
        assert_eq!(discovery.candidates.len(), 1);
        assert_eq!(discovery.candidates[0].area.r1, poirot_model::MAX_ROWS - 1);
    }
    #[test]
    fn frontier_strips_match_independent_coordinate_union_without_revisits() {
        let points: Vec<_> = (0..4)
            .flat_map(|row| (0..4).map(move |col| CellAddress { row, col }))
            .collect();
        for a in &points {
            for b in &points {
                for c in &points {
                    let mut discovery = Discovery::default();
                    let mut visited = std::collections::BTreeSet::new();
                    let mut budget = BudgetMeter::new(Default::default()).unwrap();
                    for last in [*a, *b, *c] {
                        let Some(expansion) =
                            discovery.expansion(0, last, &mut budget, None).unwrap()
                        else {
                            continue;
                        };
                        for strip in expansion.strips {
                            for row in strip.first.row..=strip.last.row {
                                for col in strip.first.col..=strip.last.col {
                                    assert!(
                                        visited.insert(CellAddress { row, col }),
                                        "source revisited"
                                    );
                                }
                            }
                        }
                        for column in expansion.removed_columns {
                            discovery.inspected.remove(&column);
                        }
                        discovery.inspected.insert(last.col, last.row);
                        let rows: Vec<_> = discovery.inspected.values().copied().collect();
                        assert!(rows.windows(2).all(|w| w[0] > w[1]));
                    }
                    for p in &points {
                        let expected = [*a, *b, *c]
                            .iter()
                            .any(|end| end.row >= p.row && end.col >= p.col);
                        assert_eq!(visited.contains(p), expected);
                        assert_eq!(discovery.covers(*p), expected);
                    }
                }
            }
        }
    }
    #[test]
    fn refused_expansion_keeps_prior_shared_completeness_and_never_publishes_partial_sources() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        sheet.set_value(CellAddress { row: 0, col: 0 }, 1.).unwrap();
        sheet
            .set_formula(CellAddress { row: 0, col: 2 }, "SEQUENCE(2)")
            .unwrap();
        let shared = std::cell::RefCell::new(CandidateIndex::new());
        let mut refused = Session::with_context(&book, Default::default());
        refused.spill_candidates = Some(&shared);
        refused.budget = BudgetMeter::new(EvaluationBudgets {
            materialized_values: 0,
            ..Default::default()
        })
        .unwrap();
        assert!(
            refused
                .discover_candidates(0, CellAddress { row: 0, col: 0 })
                .is_err()
        );
        assert!(
            shared.borrow().is_empty(),
            "a failed first scan cannot publish an unbuilt legacy set"
        );
        let mut session = Session::with_context(&book, Default::default());
        session.spill_candidates = Some(&shared);
        let first = CellAddress { row: 1, col: 0 };
        session.discover_candidates(0, first).unwrap();
        let initial = shared.borrow().get(&0).unwrap().inspected.clone();
        session.runtime.budget = BudgetMeter::new(EvaluationBudgets {
            materialized_values: 1,
            ..Default::default()
        })
        .unwrap();
        assert!(
            session
                .discover_candidates(0, CellAddress { row: 0, col: 2 })
                .is_err()
        );
        assert_eq!(shared.borrow().get(&0).unwrap().inspected, initial);
        assert!(shared.borrow().get(&0).unwrap().covers(first));
        assert!(
            !shared
                .borrow()
                .get(&0)
                .unwrap()
                .covers(CellAddress { row: 0, col: 2 })
        );
        let mut fresh = Session::with_context(&book, Default::default());
        fresh.spill_candidates = Some(&shared);
        fresh.discover_candidates(0, first).unwrap();
        assert_eq!(shared.borrow().get(&0).unwrap().inspected, initial);
        fresh
            .discover_candidates(0, CellAddress { row: 0, col: 2 })
            .unwrap();
        let expanded = shared.borrow();
        let expanded = expanded.get(&0).unwrap();
        assert!(expanded.covers(first));
        assert_eq!(expanded.candidates.len(), 1);
        assert_eq!(
            expanded.candidates[0].anchor,
            CellAddress { row: 0, col: 2 }
        );
    }
}

#[cfg(test)]
mod access_api_tests {
    use super::*;

    #[test]
    fn publication_requires_successful_settlement_and_acknowledges_without_reallocating() {
        let key = (0, CellAddress { row: 0, col: 0 });
        let mut preparation = Preparation::default();
        preparation
            .roles
            .insert(key, SpillRole::Anchor { last: key.1 });
        preparation.journal.push(key);
        let capacity = preparation.journal.capacity();
        preparation.settlement.begin(Default::default(), 0, 0);
        assert!(preparation.publication().is_err());
        preparation.settlement.finish();
        let publication = preparation.publication().unwrap();
        assert_eq!(publication.pending_cells(), &[key]);
        assert_eq!(publication.extent(key), Some(key.1));
        assert_eq!(publication.role(key).owner(key.1), Some(key.1));
        assert_eq!(publication.generated_count(), 1);
        assert!(publication.children().next().is_none());
        // Merely borrowing a batch, including an abandoned one, cannot consume it.
        assert_eq!(preparation.publication().unwrap().pending_cells(), &[key]);
        preparation.acknowledge_publication();
        assert!(
            preparation
                .publication()
                .unwrap()
                .pending_cells()
                .is_empty()
        );
        assert_eq!(preparation.journal.capacity(), capacity);
        preparation.failure = Some(EvaluationFailure::StalePlan);
        assert!(preparation.publication().is_err());
        assert!(
            matches!(preparation.admitted_role(key), SpillRole::Anchor { last } if last == key.1)
        );
    }

    #[test]
    fn evaluated_roles_distinguish_one_cell_error_blocked_and_child_results() {
        let mut book = Workbook::new();
        let sheet = book.add_sheet("S").unwrap();
        for (address, formula) in [
            ("A1", "{1}"),
            ("C1", "{#N/A,2}"),
            ("F1", "{3,4}"),
            ("I1", "{5,6}"),
        ] {
            sheet
                .set_formula(address.parse().unwrap(), formula)
                .unwrap();
        }
        sheet.set_value("J1".parse().unwrap(), 99.).unwrap();
        let mut session = Session::with_context(&book, Default::default());
        session.array_result_policy = ArrayResultPolicy::DynamicSpill;
        for (address, expected, extent) in [
            ("A1", Number(1.), Some("A1")),
            ("C1", Error(CellError::NA), Some("D1")),
            ("G1", Number(4.), None),
            ("I1", Error(CellError::Spill), None),
        ] {
            let at: CellAddress = address.parse().unwrap();
            assert_eq!(session.cell(0, at).unwrap(), expected);
            let publication = session.runtime.preparation.publication().unwrap();
            assert_eq!(
                publication.extent((0, at)),
                extent.map(|s| s.parse().unwrap())
            );
        }
        let child: CellAddress = "G1".parse().unwrap();
        let anchor: CellAddress = "F1".parse().unwrap();
        assert!(
            matches!(session.runtime.preparation.admitted_role((0, child)), SpillRole::Child { anchor: owner } if owner == anchor)
        );
        // Evaluation does not mutate saved worksheet ownership or geometry.
        assert_eq!(book.sheet(0).unwrap().spill_owner(child), None);
        assert_eq!(book.sheet(0).unwrap().spill_extent(anchor), None);
    }

    #[test]
    fn producer_observations_share_lifecycle_and_keep_sparse_diagnostic_priority() {
        let key = (0, CellAddress { row: 0, col: 0 });
        let point = (0, CellAddress { row: 1, col: 1 });
        let other = (1, point.1);
        let area = Area {
            kind: AreaKind::Area,
            sheet: 0,
            r0: 1,
            r1: 1,
            c0: 1,
            c1: 1,
        };
        let mut preparation = Preparation::default();
        let mut budget = BudgetMeter::new(Default::default()).unwrap();
        preparation
            .producer_reads
            .record_blank(key, point, &mut budget)
            .unwrap();
        preparation
            .producer_reads
            .record_blank(key, point, &mut budget)
            .unwrap();
        assert_eq!(preparation.producer_reads.blank_points[&key].len(), 1);
        assert_eq!(
            preparation.producer_reads.self_read(key, point),
            Some(SelfRead::Blank)
        );
        assert_eq!(preparation.producer_reads.self_read(key, other), None);
        preparation
            .producer_reads
            .record_sparse(key, area, &mut budget)
            .unwrap();
        assert_eq!(
            preparation.producer_reads.self_read(key, point),
            Some(SelfRead::Sparse)
        );
        preparation.producer_reads.forget(key);
        assert_eq!(preparation.producer_reads.self_read(key, point), None);
        preparation
            .producer_reads
            .record_sparse(key, area, &mut budget)
            .unwrap();
        preparation.producer_reads.clear();
        assert!(preparation.producer_reads.blank_points.is_empty());
        assert!(preparation.producer_reads.sparse_ranges.is_empty());
    }

    #[test]
    fn preparation_reset_clears_both_observation_channels_and_reuses_their_buffers() {
        let key = (0, CellAddress { row: 0, col: 0 });
        let area = Area {
            kind: AreaKind::Area,
            sheet: 0,
            r0: 0,
            r1: 0,
            c0: 0,
            c1: 0,
        };
        let mut preparation = Preparation::default();
        let mut budget = BudgetMeter::new(Default::default()).unwrap();
        preparation
            .producer_reads
            .record_blank(key, key, &mut budget)
            .unwrap();
        preparation
            .producer_reads
            .record_sparse(key, area, &mut budget)
            .unwrap();
        let capacities = (
            preparation.producer_reads.blank_points.capacity(),
            preparation.producer_reads.sparse_ranges.capacity(),
        );
        preparation.failure = Some(EvaluationFailure::StalePlan);
        preparation
            .roles
            .insert(key, SpillRole::Anchor { last: key.1 });
        preparation.reset();
        assert!(
            preparation
                .publication()
                .unwrap()
                .pending_cells()
                .is_empty()
        );
        assert!(matches!(preparation.admitted_role(key), SpillRole::None));
        assert!(preparation.producer_reads.blank_points.is_empty());
        assert!(preparation.producer_reads.sparse_ranges.is_empty());
        assert_eq!(
            (
                preparation.producer_reads.blank_points.capacity(),
                preparation.producer_reads.sparse_ranges.capacity()
            ),
            capacities
        );
    }

    #[test]
    fn refused_self_read_recording_never_inserts_observations() {
        let key = (0, CellAddress { row: 0, col: 0 });
        let area = Area {
            kind: AreaKind::Area,
            sheet: 0,
            r0: 0,
            r1: 0,
            c0: 0,
            c1: 0,
        };
        for limits in [
            EvaluationBudgets {
                run_work: 0,
                ..Default::default()
            },
            EvaluationBudgets {
                materialized_values: 0,
                ..Default::default()
            },
        ] {
            let mut preparation = Preparation::default();
            let mut budget = BudgetMeter::new(limits).unwrap();
            assert!(
                preparation
                    .producer_reads
                    .record_blank(key, key, &mut budget)
                    .is_err()
            );
            assert!(
                preparation
                    .producer_reads
                    .record_sparse(key, area, &mut budget)
                    .is_err()
            );
            assert_eq!(preparation.producer_reads.self_read(key, key), None);
        }
    }
}
