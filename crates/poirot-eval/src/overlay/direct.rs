//! Editable direct execution: local memoization without retained dependency indexes.
use super::continuation::Continuation;
use super::*;

/// One editable evaluation over an immutable baseline; every source edit clears results.
/// Policies and bytecode persist; only an explicit new context resets work counters.
pub struct Evaluation<'a, W = &'a Workbook> {
    source: Source<'a, W>,
    context: EvaluationContext,
    state: Continuation,
    completed: crate::coordinate_hash::CellSet,
    spill_resets: std::collections::BTreeSet<Key>,
    failure: Option<EvaluationFailure>,
}
/// Builds direct evaluation from a baseline handle and source policies.
pub struct EvaluationBuilder<'a, W> {
    source: OverlayBuilder<'a, W>,
}
impl<'a, W: Borrow<Workbook>> Evaluation<'a, W> {
    /// Start configuring direct evaluation without copying the workbook.
    pub fn with_workbook(workbook: W) -> EvaluationBuilder<'a, W> {
        EvaluationBuilder {
            source: Overlay::with_workbook(workbook),
        }
    }
    /// Start local-only execution against validated source and an explicit context.
    pub(super) fn from_source(
        source: Source<'a, W>,
        context: EvaluationContext,
    ) -> Result<Self, OverlayError> {
        let meter = crate::budget::BudgetMeter::new(context.budgets)?;
        let state = Continuation::capture(source.session(&context, meter));
        Ok(Self {
            source,
            context,
            state,
            completed: Default::default(),
            spill_resets: Default::default(),
            failure: None,
        })
    }
    source_methods!();
    fn policy_changed(&mut self, _previous: &OverlayPolicy) {
        self.clear_cache();
    }
    fn source_changed(&mut self, _target: EvaluationTarget) {
        self.clear_cache();
    }
    /// Clear local values/spills after an edit; cumulative budgets and failures remain.
    pub fn clear_cache(&mut self) {
        self.state.clear_results();
        self.completed.clear();
    }
    /// Remove authored edits and local results while retaining compiled programs.
    pub fn reset(&mut self) {
        self.source.inputs = crate::run_overlay::RunInputs::empty();
        self.clear_cache();
    }
    /// Start a new evaluation identity, resetting results, work budgets, and any failure.
    pub fn begin(&mut self, context: EvaluationContext) -> Result<(), OverlayError> {
        let meter = crate::budget::BudgetMeter::new(context.budgets)?;
        let state = Continuation::capture(self.source.session(&context, meter));
        self.context = context;
        self.state = state;
        self.completed.clear();
        self.failure = None;
        Ok(())
    }
    /// Borrow the fixed context for this evaluation identity.
    pub fn context(&self) -> &EvaluationContext {
        &self.context
    }
    /// Evaluate a single output through the shared execution engine.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationFailure> {
        self.perform(|session| session.cell(target.sheet, target.address))
    }
    /// Evaluate outputs atomically in caller order, preserving duplicates.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.perform(|session| output::outputs(session, targets))
    }
    /// Evaluate sparse effective source cells and discovered spill children.
    pub fn all(&mut self) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.perform(output::all)
    }
    /// Values from successful calls, including dependencies and spill children.
    pub fn values(&self) -> impl Iterator<Item = (EvaluationTarget, &CellValue)> {
        let state = &self.state;
        self.completed.iter().filter_map(move |key| {
            state.values().get(key).map(|value| {
                (
                    EvaluationTarget {
                        sheet: key.0,
                        address: key.1,
                    },
                    value,
                )
            })
        })
    }
    /// Count values retained from successful output requests.
    pub fn cached_cells(&self) -> usize {
        self.completed.len()
    }
    /// Inspect a successful local result without evaluation.
    pub fn cached_value(&self, target: EvaluationTarget) -> Option<&CellValue> {
        let key = (target.sheet, target.address);
        if !self.completed.contains(&key) {
            return None;
        }
        self.state.values().get(&key)
    }
    /// Work accumulated across output requests and edits in this evaluation identity.
    pub fn usage(&self) -> &crate::RunUsage {
        self.state.usage()
    }
    /// Borrow serializable source and bytecode; local results are deliberately not restored.
    pub fn snapshot(&self) -> snapshot::Snapshot<'_> {
        snapshot::Snapshot::borrowed(&self.source, None)
    }
    /// Export owned authored edits and results from successful calls.
    pub fn changes(&self) -> Vec<WorkbookChange> {
        let mut changes = self
            .spill_resets
            .iter()
            .map(|&(sheet, anchor)| WorkbookChange::ClearSpillCache { sheet, anchor })
            .collect::<Vec<_>>();
        changes.extend(self.source.source_changes());
        let state = &self.state;
        let mut completed = self.completed.iter().copied().collect::<Vec<_>>();
        completed.sort_unstable();
        changes.extend(completed.into_iter().filter_map(|key| {
            state.change(
                self.source.effective(),
                key,
                self.source.policy.materialization,
            )
        }));
        changes
    }
    /// Consume the workspace and export owned source and successful result changes.
    pub fn into_changes(self) -> Vec<WorkbookChange> {
        self.changes()
    }
    /// Finish with accumulated usage and any operational failure.
    pub fn finish(self) -> crate::RunReport<()> {
        self.state.finish(self.failure)
    }
    fn perform<T>(
        &mut self,
        evaluate: impl FnOnce(&mut crate::Session<'_>) -> Result<T, EvaluationFailure>,
    ) -> Result<T, EvaluationFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let mut session = self.state.resume(&self.source, &self.context);
        let result = evaluate(&mut session);
        // The local memo grows monotonically between edits. A cache-only request
        // needs no rescan of all previously published dependencies and spill roots.
        if result.is_ok() && session.cache.len() != self.completed.len() {
            self.completed.extend(session.cache.keys().copied());
            // Scalar results must also clear saved children from an earlier spill policy.
            self.spill_resets.extend(
                session
                    .cache
                    .keys()
                    .filter(|&&(sheet, at)| {
                        self.source
                            .effective()
                            .cell(sheet, at)
                            .is_some_and(|c| c.formula.is_some())
                    })
                    .copied(),
            );
        }
        self.state = Continuation::capture(session);
        if let Err(failure) = &result {
            self.failure = Some(failure.clone());
        }
        result
    }
}
impl Evaluation<'static, &'static Workbook> {
    /// Restore a baseline and detached state, then attach with a fresh context.
    pub fn from_snapshot(
        snapshot: snapshot::Snapshot<'_>,
    ) -> Result<(Workbook, snapshot::WorkspaceState), OverlayError> {
        snapshot.restore()
    }
}
impl<'a, W: Borrow<Workbook>> EvaluationBuilder<'a, W> {
    /// Select parsing, calculation, and materialization policies.
    pub fn with_policy(mut self, policy: OverlayPolicy) -> Self {
        self.source = self.source.with_policy(policy);
        self
    }
    /// Borrow the exact runtime extension registry.
    pub fn with_extensions(mut self, registry: &'a crate::extensions::ExtensionRegistry) -> Self {
        self.source = self.source.with_extensions(registry);
        self
    }
    /// Share immutable runtime providers with worker-owned direct evaluation.
    pub fn with_shared_extensions(
        mut self,
        registry: Arc<crate::extensions::ExtensionRegistry>,
    ) -> Self {
        self.source = self.source.with_shared_extensions(registry);
        self
    }
    /// Validate policies and context and start a direct evaluation identity.
    pub fn build(self, context: EvaluationContext) -> Result<Evaluation<'a, W>, OverlayError> {
        Evaluation::from_source(
            Source::new(
                self.source.workbook,
                self.source.policy,
                self.source.extensions,
            )?,
            context,
        )
    }
}
