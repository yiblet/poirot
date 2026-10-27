//! Persistent execution owner reborrowed by temporary source and observer ports.
use super::overlay_evaluation;
use super::source::Source;
use crate::eval::config::EvaluationContext;
use crate::eval::limits::EvaluationFailure;
use crate::eval::reuse::Key;
use crate::model::{Workbook, WorkbookChange};
use std::borrow::Borrow;

/// Owned run state whose stacks, caches and budgets stay in place across outputs.
pub(super) struct Continuation {
    /// Execution storage lent to each session resumed from this owner.
    pub(super) runtime: crate::eval::session::Runtime,
}

impl Continuation {
    /// Start a persistent machine with a validated cumulative work meter.
    pub fn new(meter: crate::eval::limits::BudgetMeter) -> Self {
        let mut runtime = crate::eval::session::Runtime::new(meter);
        runtime.memo.admitted = Some(Vec::new());
        Self { runtime }
    }

    /// Attach source ports while retaining ownership of all execution storage.
    pub fn resume<'e, W: Borrow<Workbook>>(
        &'e mut self,
        source: &'e Source<'_, W>,
        context: &'e EvaluationContext,
    ) -> crate::eval::session::Session<'e> {
        source.session_with_runtime(
            context,
            crate::eval::session::Storage::Borrowed(&mut self.runtime),
        )
    }

    /// Forget derived values while retaining cumulative work budgets and counters.
    pub fn clear_results(&mut self) {
        self.runtime.memo.cache.clear();
        if let Some(admitted) = &mut self.runtime.memo.admitted {
            admitted.clear();
        }
        self.runtime.preparation.reset();
        self.runtime.stack.visiting.clear();
        self.runtime.stack.names.clear();
        self.runtime.effects.random_streams.clear();
        self.runtime.effects.numeric_date_effects.clear();
        self.runtime.effects.extension_trace = Default::default();
        self.runtime.lookup_prefixes = Default::default();
        self.runtime.lookup_prefixes.enable();
        self.runtime.memo.usage = Default::default();
        self.runtime.stack.consumer = None;
    }

    /// Successful or failed work accumulated in this evaluation identity.
    pub fn usage(&self) -> &crate::eval::limits::RunUsage {
        &self.runtime.run_usage
    }

    /// Read the local value memo; no incremental evidence is attached.
    pub fn values(&self) -> &crate::eval::coordinates::CellMemo {
        &self.runtime.memo.cache
    }

    /// Export a result selected from the direct evaluator's successful-call set.
    pub fn change(
        &self,
        source: crate::eval::inputs::EffectiveWorkbook<'_>,
        key: Key,
        policy: crate::eval::materialization::FormulaCacheMaterializationPolicy,
    ) -> Option<WorkbookChange> {
        let value = self.runtime.memo.cache.get(&key)?;
        // Direct changes selects only earlier successful-call keys. A later failed
        // settlement rolls back its own admissions without revoking those results.
        overlay_evaluation::evaluated_change(
            source,
            key,
            value,
            self.runtime.preparation.admitted_role(key),
            policy,
        )
    }

    /// Finish this evaluation with actual-work budget information.
    pub fn finish(self, failure: Option<EvaluationFailure>) -> crate::eval::run::RunReport<()> {
        crate::eval::run::RunReport {
            execution: self.runtime.run_usage,
            outcome: self.runtime.budget.outcome(failure.map_or(Ok(()), Err)),
        }
    }
}
