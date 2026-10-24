//! Persistent execution owner reborrowed by temporary source and observer ports.
use super::*;

/// Owned run state whose stacks, caches and budgets stay in place across outputs.
pub(super) struct Continuation {
    runtime: crate::runtime::Runtime,
}
impl std::ops::Deref for Continuation {
    /// Persistent machine storage retained by the workspace.
    type Target = crate::runtime::Runtime;
    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}
impl std::ops::DerefMut for Continuation {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime
    }
}
impl Continuation {
    /// Start a persistent machine with a validated cumulative work meter.
    pub fn new(meter: crate::budget::BudgetMeter) -> Self {
        let mut runtime = crate::runtime::Runtime::new(meter);
        runtime.admitted = Some(Vec::new());
        Self { runtime }
    }
    /// Attach source ports while retaining ownership of all execution storage.
    pub fn resume<'e, W: Borrow<Workbook>>(
        &'e mut self,
        source: &'e Source<'_, W>,
        context: &'e EvaluationContext,
    ) -> crate::Session<'e> {
        source.session_with_runtime(
            context,
            crate::runtime::Storage::Borrowed(&mut self.runtime),
        )
    }
    /// Forget derived values while retaining cumulative work budgets and counters.
    pub fn clear_results(&mut self) {
        self.cache.clear();
        if let Some(admitted) = &mut self.admitted {
            admitted.clear();
        }
        self.preparation = Default::default();
        self.spill_read_ranges.clear();
        self.blank_reads.clear();
        self.visiting.clear();
        self.names.clear();
        self.random_streams.clear();
        self.numeric_date_effects.clear();
        self.extension_trace = Default::default();
        self.lookup_prefixes = Default::default();
        self.lookup_prefixes.enable();
        self.memo_usage = Default::default();
        self.consumer = None;
    }
    /// Successful or failed work accumulated in this evaluation identity.
    pub fn usage(&self) -> &crate::RunUsage {
        &self.run_usage
    }
    /// Read the local value memo; no incremental evidence is attached.
    pub fn values(&self) -> &crate::run::CellMemo {
        &self.cache
    }
    /// Export one result with guarded spill ownership.
    pub fn change(
        &self,
        source: crate::run_overlay::EffectiveWorkbook<'_>,
        key: Key,
        policy: crate::FormulaCacheMaterializationPolicy,
    ) -> Option<WorkbookChange> {
        let value = self.cache.get(&key)?;
        evaluation::evaluated_change(
            source,
            key,
            value,
            self.preparation
                .roles
                .get(&key)
                .copied()
                .unwrap_or_default(),
            policy,
        )
    }
    /// Finish this evaluation with actual-work budget information.
    pub fn finish(self, failure: Option<EvaluationFailure>) -> crate::RunReport<()> {
        crate::RunReport {
            execution: self.runtime.run_usage,
            outcome: self.runtime.budget.outcome(failure.map_or(Ok(()), Err)),
        }
    }
}
