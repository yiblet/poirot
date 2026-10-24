//! Demand selection, retry, rollback and settlement through a constrained execution port.
use super::{
    Area, AreaKind, Candidate, CellAddress, CellSet, Completion, Discovery, EvalResult,
    EvaluationFailure, EvaluationTarget, Preparation,
};
use crate::{CapabilityId, run, session_execution};

/// Services required by ownership settlement, excluding evaluator configuration and observers.
///
/// Producer execution may recursively enter settlement. Every state or index borrow
/// ends before `compute`, `admit`, or `notify` calls back into the evaluator.
/// The adapter alone knows how these operations reach workbook sources and effects.
pub(super) trait Execution<'a> {
    /// Whether this run admits dynamic array results.
    fn enabled(&self) -> bool;
    /// Borrow private spill state without unrelated execution storage.
    fn state(&self) -> &Preparation;
    /// Mutate private spill state without unrelated execution storage.
    fn state_mut(&mut self) -> &mut Preparation;
    /// Whether a prior settlement already covers this demand.
    fn ready(&self, area: Area) -> bool;
    /// Whether authored point content excludes all incoming producers.
    fn source_blocks(
        &self,
        area: Area,
        source: Option<session_execution::ResolvedSource<'a>>,
    ) -> bool;
    /// Inspect and atomically retain any newly required source prefix.
    fn discover(&mut self, sheet: usize, last: CellAddress) -> EvalResult<()>;
    /// Inspect the candidate index without retaining its borrow across execution.
    fn inspect<R>(&self, sheet: usize, read: impl FnOnce(&Discovery) -> R) -> EvalResult<R>;
    /// Queue known owners or competitors within already inspected source bounds.
    fn select_known(
        &mut self,
        area: Area,
        competitor: Option<Candidate>,
        pending: &mut Vec<Candidate>,
    ) -> EvalResult<()>;
    /// Snapshot retained memo usage and admission count for possible rollback.
    fn checkpoint(&self) -> (run::MemoRetentionUsage, usize);
    /// Whether a coordinate has an admitted result in this attempt or a prior one.
    fn is_admitted(&self, key: (usize, CellAddress)) -> bool;
    /// Compute one private producer, allowing recursive settlement of dependencies.
    fn compute(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()>;
    /// Admit one computed producer while retaining geometry for arbitration.
    fn admit(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()>;
    /// Determine blocked producers from actual attempted footprints.
    fn losers(&mut self) -> EvalResult<CellSet>;
    /// Rewind only the active attempt's results, observations and random streams.
    fn rollback(&mut self);
    /// Charge the retry history before allocating its next entry.
    fn reserve_history(&mut self, count: usize) -> EvalResult<()>;
    /// Attribute a storage failure to the currently executing consumer.
    fn consumer(&self) -> Option<EvaluationTarget>;
    /// Deliver one settled completion without exposing observer handles.
    fn notify(&mut self, notification: Completion) -> EvalResult<()>;
}

/// Owns settlement transitions while borrowing only the declared execution services.
pub(super) struct Engine<'p, P> {
    execution: &'p mut P,
}
impl<'p, P> Engine<'p, P> {
    /// Attach one short-lived execution adapter; no state is copied or detached.
    pub(super) fn new(execution: &'p mut P) -> Self {
        Self { execution }
    }
}
impl<'a, P: Execution<'a>> Engine<'_, P> {
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
        source: Option<session_execution::ResolvedSource<'a>>,
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
                        .map_err(|_| EvaluationFailure::Resource {
                            kind: crate::ResourceFailureKind::Allocation,
                            capability: CapabilityId::FunctionAdapter,
                            source: consumer,
                            detail: "settled spill membership allocation failed".into(),
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
                .ok_or(EvaluationFailure::StalePlan)?;
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
                    "spill settlement would replay an actually consumed opaque or volatile provider",
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
                    "spill ownership component does not settle",
                ));
            }
            self.execution
                .reserve_history(blocked.len().saturating_add(1))?;
            seen.try_reserve(1)
                .map_err(|_| EvaluationFailure::Resource {
                    kind: crate::ResourceFailureKind::Allocation,
                    capability: CapabilityId::FunctionAdapter,
                    source: self.execution.consumer(),
                    detail: "spill settlement history allocation failed".into(),
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
            .map_err(|_| EvaluationFailure::Resource {
                kind: crate::ResourceFailureKind::Allocation,
                capability: CapabilityId::FunctionAdapter,
                source: consumer,
                detail: "settled spill membership allocation failed".into(),
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
pub(super) fn unsupported(sheet: usize, address: CellAddress, detail: &str) -> EvaluationFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        source: Some(EvaluationTarget { sheet, address }),
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CellProvenance, Number};

    /// Scripted producer outcome, independent of workbook or evaluator construction.
    enum Outcome {
        StableAfterRetry,
        Failure,
    }
    /// Minimal settlement adapter proving that orchestration needs no Session.
    struct ScriptedExecution {
        state: Preparation,
        admitted: CellSet,
        outcome: Outcome,
        events: Vec<&'static str>,
    }
    impl ScriptedExecution {
        /// Seed two independent producers so failure can follow a provisional success.
        fn new(outcome: Outcome) -> Self {
            let candidates: Vec<_> = [0, 2]
                .into_iter()
                .map(|col| {
                    Candidate::new(
                        0,
                        CellAddress { row: 0, col },
                        poirot_model::ShapeBounds::new(1, 1),
                    )
                })
                .collect();
            let mut state = Preparation::default();
            state.candidates.insert(
                0,
                Discovery {
                    candidates,
                    ending_rows: vec![0, 0],
                    ..Default::default()
                },
            );
            Self {
                state,
                admitted: Default::default(),
                outcome,
                events: Vec::new(),
            }
        }
        /// Demand both seeded producers through one ownership component.
        fn area() -> Area {
            Area {
                kind: AreaKind::Area,
                sheet: 0,
                r0: 0,
                r1: 0,
                c0: 0,
                c1: 2,
            }
        }
    }
    impl<'a> Execution<'a> for ScriptedExecution {
        fn enabled(&self) -> bool {
            true
        }
        fn state(&self) -> &Preparation {
            &self.state
        }
        fn state_mut(&mut self) -> &mut Preparation {
            &mut self.state
        }
        fn ready(&self, _area: Area) -> bool {
            false
        }
        fn source_blocks(
            &self,
            _area: Area,
            _source: Option<session_execution::ResolvedSource<'a>>,
        ) -> bool {
            false
        }
        fn discover(&mut self, _sheet: usize, _last: CellAddress) -> EvalResult<()> {
            self.events.push("discover");
            Ok(())
        }
        fn inspect<R>(&self, sheet: usize, read: impl FnOnce(&Discovery) -> R) -> EvalResult<R> {
            self.state
                .candidates
                .get(&sheet)
                .map(read)
                .ok_or(EvaluationFailure::StalePlan)
        }
        fn select_known(
            &mut self,
            _area: Area,
            _competitor: Option<Candidate>,
            _pending: &mut Vec<Candidate>,
        ) -> EvalResult<()> {
            let active = self.state.settlement.active_mut().unwrap();
            active.selected.extend(
                self.state.candidates[&0]
                    .candidates
                    .iter()
                    .map(|candidate| (0, candidate.anchor)),
            );
            Ok(())
        }
        fn checkpoint(&self) -> (run::MemoRetentionUsage, usize) {
            (Default::default(), 0)
        }
        fn is_admitted(&self, key: (usize, CellAddress)) -> bool {
            self.admitted.contains(&key)
        }
        fn compute(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
            self.events.push("compute");
            if matches!(self.outcome, Outcome::Failure) && at.col == 2 {
                self.state
                    .settlement
                    .active_mut()
                    .unwrap()
                    .notifications
                    .push(Completion {
                        consumer: None,
                        target: EvaluationTarget { sheet, address: at },
                        provenance: CellProvenance::Evaluated,
                        result: Err(EvaluationFailure::StalePlan),
                    });
                return Err(EvaluationFailure::StalePlan);
            }
            Ok(())
        }
        fn admit(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
            self.events.push("admit");
            self.admitted.insert((sheet, at));
            self.state
                .settlement
                .active_mut()
                .unwrap()
                .notifications
                .push(Completion {
                    consumer: None,
                    target: EvaluationTarget { sheet, address: at },
                    provenance: CellProvenance::Evaluated,
                    result: Ok(Number(1.)),
                });
            Ok(())
        }
        fn losers(&mut self) -> EvalResult<CellSet> {
            self.events.push("arbitrate");
            Ok(CellSet::from_iter([(0, CellAddress { row: 0, col: 2 })]))
        }
        fn rollback(&mut self) {
            self.events.push("rollback");
            self.admitted.clear();
            self.state
                .settlement
                .active_mut()
                .unwrap()
                .notifications
                .clear();
        }
        fn reserve_history(&mut self, _count: usize) -> EvalResult<()> {
            self.events.push("reserve");
            Ok(())
        }
        fn consumer(&self) -> Option<EvaluationTarget> {
            None
        }
        fn notify(&mut self, notification: Completion) -> EvalResult<()> {
            self.events.push(if notification.result.is_ok() {
                "success"
            } else {
                "failure"
            });
            Ok(())
        }
    }

    #[test]
    fn engine_retries_and_defers_notifications_without_workbook_execution() {
        let mut execution = ScriptedExecution::new(Outcome::StableAfterRetry);
        Engine::new(&mut execution)
            .settle(ScriptedExecution::area(), None)
            .unwrap();
        assert_eq!(
            execution.events,
            [
                "discover",
                "compute",
                "admit",
                "compute",
                "admit",
                "arbitrate",
                "reserve",
                "rollback",
                "compute",
                "admit",
                "compute",
                "admit",
                "arbitrate",
                "success",
                "success",
            ]
        );
        assert_eq!(execution.state.settled.len(), 2);
        assert!(execution.state.settlement.active().is_none());
        assert!(execution.state.failure.is_none());
    }

    #[test]
    fn engine_rolls_back_provisional_success_and_delivers_only_operational_failure() {
        let mut execution = ScriptedExecution::new(Outcome::Failure);
        assert!(
            Engine::new(&mut execution)
                .settle(ScriptedExecution::area(), None)
                .is_err()
        );
        assert_eq!(
            execution.events,
            [
                "discover", "compute", "admit", "compute", "failure", "rollback"
            ]
        );
        assert!(execution.admitted.is_empty());
        assert!(execution.state.settled.is_empty());
        assert!(execution.state.settlement.active().is_none());
        assert!(matches!(
            execution.state.failure,
            Some(EvaluationFailure::StalePlan)
        ));
        let before = execution.events.len();
        assert!(
            Engine::new(&mut execution)
                .settle(ScriptedExecution::area(), None)
                .is_err()
        );
        assert_eq!(execution.events.len(), before);
    }
}
