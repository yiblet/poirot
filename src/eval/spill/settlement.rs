//! Demand selection, retry, rollback and settlement through a constrained execution port.
use super::execution::Execution;
use crate::eval::coordinates::{CellSet, EvaluationTarget};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError};
use crate::eval::session;
use crate::eval::values::{Area, AreaKind};
use crate::model::CellAddress;

/// Settles the array producers that can affect a requested rectangle.
/// It schedules producers, retries conflicting footprints, and rolls back failed
/// attempts through an execution port rather than owning an evaluator session.
pub(super) struct Settlement<'p, P> {
    execution: &'p mut P,
}

impl<'p, P> Settlement<'p, P> {
    /// Attach one short-lived execution adapter; no state is copied or detached.
    pub(super) fn new(execution: &'p mut P) -> Self {
        Self { execution }
    }
}

impl<'a, P: Execution<'a>> Settlement<'_, P> {
    /// Include relevant producers and earlier possible competitors, without reserving cells.
    fn select_spills(&mut self, area: Area) -> EvalResult<()> {
        // resolve_spills_in inspected the direct-demand prefix before selection.
        let mut pending = Vec::new();
        self.execution.select_known(area, None, &mut pending)?;
        while let Some(candidate) = pending.pop() {
            // Earlier priority bounds the anchor row independently of spill height.
            self.execution.discover(
                area.sheet,
                CellAddress {
                    row: candidate.anchor.row,
                    col: candidate.area.c1,
                },
            )?;
            self.execution
                .select_known(candidate.area, Some(candidate), &mut pending)?;
        }
        Ok(())
    }

    /// Settle only demand intersections and their necessary dependency/priority component.
    pub(super) fn settle(
        &mut self,
        area: Area,
        source: Option<session::ResolvedSource<'a>>,
    ) -> EvalResult<()> {
        if !self.execution.enabled() {
            return Ok(());
        }
        if let Some(failure) = &self.execution.state().failure {
            return Err(failure.clone());
        }
        if self.execution.ready(area) {
            return Ok(());
        }
        // Authored nonblank values and nonspilling formulas block every other
        // producer. Their point values therefore need no origin-prefix search.
        // Derived saved children still require ownership reconciliation.
        if area.kind == AreaKind::Cell && self.execution.source_blocks(area, source) {
            return Ok(());
        }
        match self.execution.discover(
            area.sheet,
            CellAddress {
                row: area.r1,
                col: area.c1,
            },
        ) {
            Ok(()) => (),
            Err(failure) => {
                self.execution.state_mut().failure = Some(failure.clone());
                return Err(failure);
            }
        };
        let state = self.execution.state();
        if self.execution.inspect(area.sheet, |p| {
            p.possible_in(area, None)
                .iter()
                .all(|c| !c.intersects(area) || state.settled.contains(&(area.sheet, c.anchor)))
        })? {
            return Ok(());
        }
        let outer = self.execution.state().settlement.active().is_none();
        if outer {
            let (memo_usage, admitted_len) = self.execution.checkpoint();
            let state = self.execution.state_mut();
            state
                .settlement
                .begin(memo_usage, state.journal.len(), admitted_len);
        }
        let mut result = self.select_spills(area).and_then(|()| {
            if outer {
                self.settle_component(area)
            } else {
                self.settle_dependencies(area)
            }
        });
        if outer {
            // Completion delivery can fail. Commit membership only after it succeeds;
            // reserve locally as well as before delivery so this final write is safe.
            if result.is_ok() {
                let consumer = self.execution.consumer();
                let state = self.execution.state_mut();
                let count = state
                    .settlement
                    .active()
                    .map_or(0, |active| active.selected.len());
                result =
                    state
                        .settled
                        .try_reserve(count)
                        .map_err(|reservation_source| EvaluationError::Resource {
                            kind: crate::eval::limits::ResourceFailureKind::Allocation,
                            capability: CapabilityId::FunctionAdapter,
                            target: consumer,
                            reason: (crate::eval::limits::EvaluationReasonError::SettledSpillMembershipAllocationFailed).with_reservation_source(reservation_source),
                        });
            }
            if let Err(failure) = &result {
                let mut notifications = self
                    .execution
                    .state_mut()
                    .settlement
                    .active_mut()
                    .map(|active| std::mem::take(&mut active.notifications))
                    .unwrap_or_default();
                for notification in notifications.drain(..).filter(|n| n.result.is_err()) {
                    let _ = self.execution.notify(notification);
                }
                if let Some(active) = self.execution.state_mut().settlement.active_mut() {
                    active.notifications = notifications;
                }
                self.execution.rollback();
                self.execution.state_mut().failure = Some(failure.clone());
            } else {
                let state = self.execution.state_mut();
                if let Some(active) = state.settlement.active() {
                    // SAFETY: This function reserved all selected slots above;
                    // neither branch mutates membership before this integer-key extension.
                    state.settled.extend(active.selected.iter().copied());
                }
            }
            self.execution.state_mut().settlement.finish();
        }
        result
    }

    /// Resolve recursive dependencies without committing the enclosing component.
    fn settle_dependencies(&mut self, area: Area) -> EvalResult<()> {
        let mut cursor = None;
        loop {
            let next = self.execution.inspect(area.sheet, |p| {
                let first = cursor.map_or(0, |at| p.candidates.partition_point(|c| c.anchor <= at));
                p.candidates
                    .get(first..)
                    .and_then(|rest| rest.iter().find(|c| c.intersects(area)))
                    .copied()
            })?;
            let Some(c) = next else {
                break;
            };
            cursor = Some(c.anchor);
            // No candidate-index borrow survives recursive computation.
            if !self
                .execution
                .state()
                .computing
                .contains(&(area.sheet, c.anchor))
            {
                self.execution.compute(area.sheet, c.anchor)?;
                self.execution.admit(area.sheet, c.anchor)?;
            }
        }
        Ok(())
    }

    /// Retry a complete component until actual footprints agree with its blocked set.
    fn settle_component(&mut self, area: Area) -> EvalResult<()> {
        let initial_selected = self
            .execution
            .state()
            .settlement
            .active()
            .map_or(0, |active| active.selected.len());
        let mut seen: Vec<(usize, CellSet)> = Vec::new();
        loop {
            // Dependencies may enlarge the selected set while one producer runs.
            loop {
                let next = self.execution.state().settlement.active().and_then(|a| {
                    a.selected
                        .iter()
                        .copied()
                        .find(|key| !self.execution.is_admitted(*key))
                });
                let Some((sheet, at)) = next else {
                    break;
                };
                self.execution.compute(sheet, at)?;
                self.execution.admit(sheet, at)?;
            }
            let blocked = self.execution.losers()?;
            let active = self
                .execution
                .state()
                .settlement
                .active()
                .ok_or(EvaluationError::StalePlan)?;
            if blocked == active.blocked {
                break;
            }
            if active.non_repeatable {
                return Err(unsupported(
                    area.sheet,
                    CellAddress {
                        row: area.r0,
                        col: area.c0,
                    },
                    crate::eval::limits::EvaluationReasonError::SpillSettlementWouldReplayAnActuallyConsumedOpaqueOrVolatileProvider,
                ));
            }
            // Selection only grows, so its count identifies its exact set
            // within this component. A changed closure is a different state.
            let selected = active.selected.len();
            if (selected == initial_selected && blocked.is_empty())
                || seen
                    .iter()
                    .any(|(size, previous)| *size == selected && *previous == blocked)
            {
                return Err(unsupported(
                    area.sheet,
                    CellAddress {
                        row: area.r0,
                        col: area.c0,
                    },
                    crate::eval::limits::EvaluationReasonError::SpillOwnershipComponentDoesNotSettle,
                ));
            }
            self.execution
                .reserve_history(blocked.len().saturating_add(1))?;
            seen.try_reserve(1)
                .map_err(|reservation_source| EvaluationError::Resource {
                    kind: crate::eval::limits::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    target: self.execution.consumer(),
                    reason: (crate::eval::limits::EvaluationReasonError::SpillSettlementHistoryAllocationFailed).with_reservation_source(reservation_source),
                })?;
            // SAFETY: The successful reservation above provides one history slot.
            // Cloning blocked reproduces an already representable integer-key table.
            seen.push((selected, blocked.clone()));
            self.execution.rollback();
            if let Some(active) = self.execution.state_mut().settlement.active_mut() {
                active.blocked = blocked;
            }
        }
        // Reserve final membership before exposing successful completions.
        let consumer = self.execution.consumer();
        let state = self.execution.state_mut();
        let count = state
            .settlement
            .active()
            .map_or(0, |active| active.selected.len());
        state
            .settled
            .try_reserve(count)
            .map_err(|reservation_source| EvaluationError::Resource {
                kind: crate::eval::limits::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                target: consumer,
                reason: (crate::eval::limits::EvaluationReasonError::SettledSpillMembershipAllocationFailed).with_reservation_source(reservation_source),
            })?;
        let notifications = self
            .execution
            .state_mut()
            .settlement
            .active_mut()
            .map(|a| std::mem::take(&mut a.notifications))
            .unwrap_or_default();
        for notification in notifications {
            self.execution.notify(notification)?;
        }
        Ok(())
    }
}

/// Attribute an unsupported spill capability to its producer cell.
pub(in crate::eval) fn unsupported(
    sheet: usize,
    address: CellAddress,
    reason: crate::eval::limits::EvaluationReasonError,
) -> EvaluationError {
    EvaluationError::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        target: Some(EvaluationTarget { sheet, address }),
        reason,
    }
}
