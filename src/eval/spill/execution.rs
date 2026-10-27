//! The settlement engine's intentionally limited execution port.
use super::completion::Completion;
use super::producer_candidate::ProducerCandidate;
use super::sheet_candidates::SheetCandidates;
use super::spill_preparation::SpillPreparation;
use crate::eval::coordinates::{CellSet, EvaluationTarget};
use crate::eval::limits::{EvalResult, MemoRetentionUsage};
use crate::eval::session;
use crate::eval::values::Area;
use crate::model::CellAddress;

/// Services required by ownership settlement, excluding evaluator configuration and observers.
///
/// Producer execution may recursively enter settlement. Every state or index borrow
/// ends before `compute`, `admit`, or `notify` calls back into the evaluator.
/// The adapter alone knows how these operations reach workbook sources and effects.
pub(super) trait Execution<'a> {
    /// Whether this run admits dynamic array results.
    fn enabled(&self) -> bool;

    /// Borrow private spill state without unrelated execution storage.
    fn state(&self) -> &SpillPreparation;

    /// Mutate private spill state without unrelated execution storage.
    fn state_mut(&mut self) -> &mut SpillPreparation;

    /// Whether a prior settlement already covers this demand.
    fn ready(&self, area: Area) -> bool;

    /// Whether authored point content excludes all incoming producers.
    fn source_blocks(&self, area: Area, source: Option<session::ResolvedSource<'a>>) -> bool;

    /// Inspect and atomically retain any newly required source prefix.
    fn discover(&mut self, sheet: usize, last: CellAddress) -> EvalResult<()>;

    /// Inspect the candidate index without retaining its borrow across execution.
    fn inspect<R>(&self, sheet: usize, read: impl FnOnce(&SheetCandidates) -> R) -> EvalResult<R>;

    /// Queue known owners or competitors within already inspected source bounds.
    fn select_known(
        &mut self,
        area: Area,
        competitor: Option<ProducerCandidate>,
        pending: &mut Vec<ProducerCandidate>,
    ) -> EvalResult<()>;

    /// Snapshot retained memo usage and admission count for possible rollback.
    fn checkpoint(&self) -> (MemoRetentionUsage, usize);

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
