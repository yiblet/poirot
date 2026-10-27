use super::super::completion::Completion;
use super::super::execution::Execution;
use super::super::producer_candidate::ProducerCandidate;
use super::super::settlement::Settlement;
use super::super::sheet_candidates::SheetCandidates;
use super::super::spill_preparation::SpillPreparation;
use crate::eval::coordinates::{CellSet, EvaluationTarget};
use crate::eval::limits::{EvalResult, EvaluationError, MemoRetentionUsage};
use crate::eval::ports::CellProvenance;
use crate::eval::session;
use crate::eval::values::{Area, AreaKind};
use crate::model::CellAddress;
use crate::model::CellValue::Number;

/// Scripted producer outcome, independent of workbook or evaluator construction.
enum Outcome {
    StableAfterRetry,
    Failure,
}

/// Minimal settlement adapter proving that orchestration needs no Session.
struct ScriptedExecution {
    state: SpillPreparation,
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
                ProducerCandidate::new(
                    0,
                    CellAddress { row: 0, col },
                    crate::model::ShapeBounds::new(1, 1),
                )
            })
            .collect();
        let mut state = SpillPreparation::default();
        state.candidates.insert(
            0,
            SheetCandidates {
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

    fn state(&self) -> &SpillPreparation {
        &self.state
    }

    fn state_mut(&mut self) -> &mut SpillPreparation {
        &mut self.state
    }

    fn ready(&self, _area: Area) -> bool {
        false
    }

    fn source_blocks(&self, _area: Area, _source: Option<session::ResolvedSource<'a>>) -> bool {
        false
    }

    fn discover(&mut self, _sheet: usize, _last: CellAddress) -> EvalResult<()> {
        self.events.push("discover");
        Ok(())
    }

    fn inspect<R>(&self, sheet: usize, read: impl FnOnce(&SheetCandidates) -> R) -> EvalResult<R> {
        self.state
            .candidates
            .get(&sheet)
            .map(read)
            .ok_or(EvaluationError::StalePlan)
    }

    fn select_known(
        &mut self,
        _area: Area,
        _competitor: Option<ProducerCandidate>,
        _pending: &mut Vec<ProducerCandidate>,
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

    fn checkpoint(&self) -> (MemoRetentionUsage, usize) {
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
                    result: Err(EvaluationError::StalePlan),
                });
            return Err(EvaluationError::StalePlan);
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
    Settlement::new(&mut execution)
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
        Settlement::new(&mut execution)
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
        Some(EvaluationError::StalePlan)
    ));
    let before = execution.events.len();
    assert!(
        Settlement::new(&mut execution)
            .settle(ScriptedExecution::area(), None)
            .is_err()
    );
    assert_eq!(execution.events.len(), before);
}
