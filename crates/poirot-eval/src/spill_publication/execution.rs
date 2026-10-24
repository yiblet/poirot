//! Evaluator adapter for the settlement engine's intentionally limited execution port.
//!
//! Formula execution and observer delivery stay here because either can call
//! external services. The engine receives neither their handles nor Session.
use super::*;
use settlement::Execution;

impl<'a> Execution<'a> for Session<'a> {
    fn enabled(&self) -> bool {
        self.array_result_policy == ArrayResultPolicy::DynamicSpill
    }
    fn state(&self) -> &Preparation {
        &self.runtime.preparation
    }
    fn state_mut(&mut self) -> &mut Preparation {
        &mut self.runtime.preparation
    }
    fn ready(&self, area: Area) -> bool {
        self.ready_area(area)
    }
    fn source_blocks(
        &self,
        area: Area,
        source: Option<session_execution::ResolvedSource<'a>>,
    ) -> bool {
        let at = CellAddress {
            row: area.r0,
            col: area.c0,
        };
        source
            .or_else(|| self.resolve_source(area.sheet, at))
            .is_some_and(|source| {
                !source.accepts_spill
                    && source
                        .cell
                        .is_some_and(|cell| match cell.formula.as_deref() {
                            Some(formula) => {
                                self.source_spill_shape(area.sheet, at, formula).is_none()
                            }
                            None => cell.value != Blank,
                        })
            })
    }
    fn discover(&mut self, sheet: usize, last: CellAddress) -> EvalResult<()> {
        self.discover_candidates(sheet, last)
    }
    fn inspect<R>(&self, sheet: usize, read: impl FnOnce(&Discovery) -> R) -> EvalResult<R> {
        self.with_discovery(sheet, read)
    }
    fn select_known(
        &mut self,
        area: Area,
        competitor: Option<Candidate>,
        pending: &mut Vec<Candidate>,
    ) -> EvalResult<()> {
        let runtime = &mut *self.runtime;
        discovery::Selection {
            preparation: &mut runtime.preparation,
            shared: self.spill_candidates,
            budget: &mut runtime.budget,
            consumer: runtime.consumer,
        }
        .select(area, competitor, pending)
    }
    fn checkpoint(&self) -> (run::MemoRetentionUsage, usize) {
        (
            self.runtime.memo_usage,
            self.runtime.admitted.as_ref().map_or(0, Vec::len),
        )
    }
    fn is_admitted(&self, key: (usize, CellAddress)) -> bool {
        self.runtime.cache.contains_key(&key)
    }
    fn compute(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        self.compute_producer(sheet, at)
    }
    fn admit(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        self.finalize_producer(sheet, at)
    }
    fn losers(&mut self) -> EvalResult<CellSet> {
        let runtime = &mut *self.runtime;
        runtime
            .preparation
            .losers(&mut runtime.budget, runtime.consumer)
    }
    fn rollback(&mut self) {
        self.rollback_spills();
    }
    fn reserve_history(&mut self, count: usize) -> EvalResult<()> {
        self.runtime.budget.allocation(count)
    }
    fn consumer(&self) -> Option<EvaluationTarget> {
        self.runtime.consumer
    }
    fn notify(&mut self, notification: Completion) -> EvalResult<()> {
        if let Some(observer) = &mut self.cell_observer {
            observer.completed(CellCompletion {
                consumer: notification.consumer,
                target: notification.target,
                provenance: notification.provenance,
                result: notification.result.as_ref(),
            })?;
        }
        Ok(())
    }
}
