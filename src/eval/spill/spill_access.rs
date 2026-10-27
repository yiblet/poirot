//! Evaluator adapter for the settlement engine's intentionally limited execution port.
//!
//! Formula execution and observer delivery stay here because either can call
//! external services. The engine receives neither their handles nor Session.
use super::completion::Completion;
use super::discovery;
use super::execution::Execution;
use super::producer_candidate::ProducerCandidate;
use super::producer_execution::ProducerExecution;
use super::producer_selection::ProducerSelection;
use super::sheet_candidates::SheetCandidates;
use super::spill_preparation::SpillPreparation;
use super::spill_query::SpillQuery;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::{CellSet, EvaluationTarget};
use crate::eval::limits::{EvalResult, MemoRetentionUsage};
use crate::eval::ports::CellCompletion;
use crate::eval::session;
use crate::eval::session::Session;
use crate::eval::values::Area;
use crate::model::CellAddress;
use crate::model::CellValue::Blank;

/// Gives the settlement engine its execution port over one borrowed session.
///
/// Each settlement attaches a fresh adapter; producer execution may recursively
/// settle again through another adapter over the same reborrowed session.
pub(super) struct SpillAccess<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> SpillAccess<'s, 'a> {
    /// Borrow the session for one settlement or discovery step.
    pub(super) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Inspect the full source domain for existing candidate-geometry unit tests.
    #[cfg(test)]
    pub(super) fn producer_candidates(
        &mut self,
        sheet: usize,
    ) -> EvalResult<Vec<ProducerCandidate>> {
        self.discover(
            sheet,
            CellAddress {
                row: crate::model::MAX_ROWS - 1,
                col: crate::model::MAX_COLUMNS - 1,
            },
        )?;
        self.inspect(sheet, |p| p.candidates.clone())
    }
}

impl<'a> Execution<'a> for SpillAccess<'_, 'a> {
    fn enabled(&self) -> bool {
        self.session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
    }

    fn state(&self) -> &SpillPreparation {
        &self.session.state.runtime.preparation
    }

    fn state_mut(&mut self) -> &mut SpillPreparation {
        &mut self.session.state.runtime.preparation
    }

    fn ready(&self, area: Area) -> bool {
        SpillQuery::new(self.session).ready(area)
    }

    fn source_blocks(&self, area: Area, source: Option<session::ResolvedSource<'a>>) -> bool {
        let at = CellAddress {
            row: area.r0,
            col: area.c0,
        };
        source
            .or_else(|| session::ResolvedSource::resolve(self.session, area.sheet, at))
            .is_some_and(|source| {
                !source.accepts_spill
                    && source
                        .cell
                        .is_some_and(|cell| match cell.formula.as_deref() {
                            Some(formula) => self
                                .session
                                .sources
                                .source_spill_shape(area.sheet, at, formula)
                                .is_none(),
                            None => cell.value != Blank,
                        })
            })
    }

    /// Discover source candidates without granting formula execution or observer access.
    fn discover(&mut self, sheet: usize, last: CellAddress) -> EvalResult<()> {
        let session = &mut *self.session;
        let source_preparation = session.sources.source_preparation();
        let runtime = &mut *session.state.runtime;
        discovery::Discovery {
            workbook: session.sources.workbook,
            inputs: session.sources.inputs,
            plan: session.sources.plan,
            source_preparation,
            shared: session.reuse.spill_candidates,
            local: &mut runtime.preparation.candidates,
            budget: &mut runtime.budget,
            consumer: runtime.stack.consumer,
        }
        .discover(sheet, last)
    }

    fn inspect<R>(&self, sheet: usize, read: impl FnOnce(&SheetCandidates) -> R) -> EvalResult<R> {
        SpillQuery::new(self.session).inspect(sheet, read)
    }

    fn select_known(
        &mut self,
        area: Area,
        competitor: Option<ProducerCandidate>,
        pending: &mut Vec<ProducerCandidate>,
    ) -> EvalResult<()> {
        let session = &mut *self.session;
        let runtime = &mut *session.state.runtime;
        ProducerSelection {
            preparation: &mut runtime.preparation,
            shared: session.reuse.spill_candidates,
            budget: &mut runtime.budget,
            consumer: runtime.stack.consumer,
        }
        .select(area, competitor, pending)
    }

    fn checkpoint(&self) -> (MemoRetentionUsage, usize) {
        (
            self.session.state.runtime.memo.usage,
            self.session
                .state
                .runtime
                .memo
                .admitted
                .as_ref()
                .map_or(0, Vec::len),
        )
    }

    fn is_admitted(&self, key: (usize, CellAddress)) -> bool {
        self.session.state.runtime.memo.cache.contains_key(&key)
    }

    fn compute(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        ProducerExecution::new(self.session).compute(sheet, at)
    }

    fn admit(&mut self, sheet: usize, at: CellAddress) -> EvalResult<()> {
        ProducerExecution::new(self.session).finalize(sheet, at)
    }

    fn losers(&mut self) -> EvalResult<CellSet> {
        let runtime = &mut *self.session.state.runtime;
        runtime
            .preparation
            .losers(&mut runtime.budget, runtime.stack.consumer)
    }

    /// Discard this attempt's admissions and rewind only newly entered cell streams.
    fn rollback(&mut self) {
        let session = &mut *self.session;
        let runtime = &mut *session.state.runtime;
        let Some(active) = runtime.preparation.settlement.active_mut() else {
            return;
        };
        for key in active.admitted.drain() {
            runtime.memo.cache.remove(&key);
            runtime.preparation.roles.remove(&key);
        }
        for key in active.random_touched.drain() {
            runtime.effects.random_streams.remove(&key);
        }
        runtime.memo.usage = active.memo_usage;
        runtime.preparation.journal.truncate(active.journal_len);
        if let Some(admitted) = &mut runtime.memo.admitted {
            admitted.truncate(active.admitted_len);
        }
        if let Some(reads) = &mut session.reuse.overlay_reads {
            reads.discard_consumers(&active.consumers);
        }
        runtime.preparation.held.clear();
        active.consumers.clear();
        active.footprints.clear();
        active.notifications.clear();
        runtime.preparation.producer_reads.clear();
    }

    fn reserve_history(&mut self, count: usize) -> EvalResult<()> {
        self.session.state.runtime.budget.allocation(count)
    }

    fn consumer(&self) -> Option<EvaluationTarget> {
        self.session.state.runtime.stack.consumer
    }

    fn notify(&mut self, notification: Completion) -> EvalResult<()> {
        if let Some(observer) = &mut self.session.ports.cell_observer {
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
