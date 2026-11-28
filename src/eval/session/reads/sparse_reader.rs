//! Sparse range membership: discovery guards, occupied-coordinate collection and members.

use super::read_recorder::ReadRecorder;
use super::sparse_member::SparseMember;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::limits::{BudgetMeter, CapabilityId, EvalResult, EvaluationError, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::composition::Session;
use crate::eval::spill::{SpillDemand, SpillQuery};
use crate::eval::values::Area;
use crate::model::CellAddress;
use std::collections::HashSet;

/// Collects occupied range members after establishing spill discovery and dependency guards.
pub(in crate::eval) struct SparseReader<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> SparseReader<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Discover spill values and record membership before collecting occupied coordinates.
    pub(in crate::eval) fn sparse_addresses(&mut self, area: Area) -> EvalResult<Vec<CellAddress>> {
        self.prepare_sparse_area(area)?;
        self.collect_sparse_addresses(area, 0)
    }

    /// Establish full-area discovery and dependency guards before any sparse read.
    pub(in crate::eval) fn prepare_sparse_area(&mut self, area: Area) -> EvalResult<()> {
        SpillDemand::new(self.session).prepare(area)?;
        let settling = SpillQuery::new(self.session).settling();
        let runtime = &mut *self.session.state.runtime;
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && settling
            && let Some(consumer) = runtime.stack.consumer
        {
            for owner in runtime
                .stack
                .visiting
                .iter()
                .copied()
                .chain(std::iter::once((consumer.sheet, consumer.address)))
            {
                runtime.preparation.producer_reads.record_sparse(
                    owner,
                    area,
                    &mut runtime.budget,
                )?;
            }
        }
        ReadRecorder::new(self.session).read(DependencyRead::SparseMembership {
            sheet: area.sheet,
            first: CellAddress {
                row: area.r0,
                col: area.c0,
            },
            last: CellAddress {
                row: area.r1,
                col: area.c1,
            },
        });
        Ok(())
    }

    /// Collect occupied coordinates after the caller established its full-area guards.
    pub(in crate::eval) fn collect_sparse_addresses(
        &mut self,
        area: Area,
        prefix_members: usize,
    ) -> EvalResult<Vec<CellAddress>> {
        self.collect_sparse(
            area,
            prefix_members,
            Vec::new(),
            |address, _| address,
            |address| *address,
        )
    }

    /// Preserve authored cells from the membership prepass for internal value consumers.
    pub(in crate::eval) fn sparse_members(
        &mut self,
        area: Area,
    ) -> EvalResult<Vec<SparseMember<'a>>> {
        self.prepare_sparse_area(area)?;
        let members = std::mem::take(&mut self.session.reuse.sparse_members);
        self.collect_sparse(
            area,
            0,
            members,
            |address, cell| SparseMember { address, cell },
            |member| member.address,
        )
    }

    /// Retain bounded member storage after the caller has consumed this traversal.
    pub(super) fn recycle_sparse_members(&mut self, mut members: Vec<SparseMember<'a>>) {
        const MAX_RECYCLED_MEMBERS: usize = 4096;
        let spare = &mut self.session.reuse.sparse_members;
        if members.capacity() <= MAX_RECYCLED_MEMBERS && members.capacity() > spare.capacity() {
            members.clear();
            *spare = members;
        }
    }

    /// Admit the complete member batch before any referenced body executes.
    fn collect_sparse<T>(
        &mut self,
        area: Area,
        prefix_members: usize,
        mut addresses: Vec<T>,
        project: impl Fn(CellAddress, Option<&'a crate::model::Cell>) -> T,
        coordinate: impl Fn(&T) -> CellAddress,
    ) -> EvalResult<Vec<T>> {
        let runtime = &mut *self.session.state.runtime;
        let membership = crate::model::query::RangeMembership::new(
            area.sheet,
            CellAddress {
                row: area.r0,
                col: area.c0,
            },
            CellAddress {
                row: area.r1,
                col: area.c1,
            },
        )
        .map_err(|_| EvaluationError::StalePlan)?;
        let view = crate::eval::inputs::EffectiveWorkbook::new(
            self.session.sources.workbook,
            self.session.sources.inputs,
        );
        // Admit each coordinate before growing the private collection. Finish
        // collecting before consumers execute any referenced cell bodies.
        let mut cells = view
            .stored(membership)
            .map_err(|_| EvaluationError::StalePlan)?;
        if !runtime.budget.tracks_work() && runtime.budget.limits.materialized_values == usize::MAX
        {
            // An unmetered run still reserves the actual storage fallibly.
            for (address, cell) in cells {
                push_authored(
                    &mut addresses,
                    *address,
                    cell,
                    &project,
                    runtime.stack.consumer,
                )?;
            }
        } else {
            while let Some((address, cell)) = sparse_next(&mut cells, &mut runtime.budget)? {
                runtime.budget.check_allocation(
                    prefix_members
                        .saturating_add(addresses.len())
                        .saturating_add(1),
                )?;
                runtime.budget.charge(WorkKind::Materialization, 1)?;
                push_authored(
                    &mut addresses,
                    address,
                    cell,
                    &project,
                    runtime.stack.consumer,
                )?;
            }
        }
        // Effective stored coordinates are already unique and row-major. Only
        // merging generated spill cells can disturb that order or add duplicates.
        if self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
            && runtime.preparation.admitted_cells().len() != 0
        {
            let mut seen: HashSet<_> = addresses.iter().map(&coordinate).collect();
            for &(sheet, address) in runtime.preparation.admitted_cells() {
                runtime.budget.charge(WorkKind::SparseScan, 1)?;
                if sheet == area.sheet && area.contains(address) && !seen.contains(&address) {
                    runtime.budget.check_allocation(
                        prefix_members
                            .saturating_add(addresses.len())
                            .saturating_add(1),
                    )?;
                    runtime.budget.charge(WorkKind::Materialization, 1)?;
                    seen.insert(address);
                    addresses.push(project(address, None));
                }
            }
            addresses.sort_by_key(&coordinate);
            addresses.dedup_by_key(|member| coordinate(member));
        }
        Ok(addresses)
    }
}

/// Append an authored member without growing the working collection infallibly.
#[inline]
fn push_authored<'a, T>(
    addresses: &mut Vec<T>,
    address: CellAddress,
    cell: &'a crate::model::Cell,
    project: &impl Fn(CellAddress, Option<&'a crate::model::Cell>) -> T,
    target: Option<crate::eval::coordinates::EvaluationTarget>,
) -> EvalResult<()> {
    addresses
        .try_reserve(1)
        .map_err(|reservation_source| EvaluationError::Resource {
            kind: crate::eval::limits::ResourceFailureKind::Allocation,
            capability: CapabilityId::FunctionAdapter,
            target,
            reason: (crate::eval::limits::EvaluationReasonError::SparseCoordinateAllocationFailed)
                .with_reservation_source(reservation_source),
        })?;
    // SAFETY: The successful reservation provides room for this one push.
    addresses.push(project(address, Some(cell)));
    Ok(())
}

/// Charge seeks and visits after every iterator step, including the terminating step.
/// Empty rows/columns can require seeks even when no stored cell is returned.
fn sparse_next<'a>(
    cells: &mut crate::eval::inputs::EffectiveStoredCells<'a>,
    budget: &mut BudgetMeter,
) -> EvalResult<Option<(CellAddress, &'a crate::model::Cell)>> {
    if !budget.tracks_work() {
        return Ok(cells.next().map(|(address, cell)| (*address, cell)));
    }
    let before = cells.counters();
    let address = cells.next().map(|(address, cell)| (*address, cell));
    let after = cells.counters();
    for amount in [
        after.row_seeks - before.row_seeks,
        after.column_seeks - before.column_seeks,
        after.cells_visited - before.cells_visited,
    ] {
        budget.charge(WorkKind::SparseScan, amount)?;
    }
    Ok(address)
}
