//! Query spreadsheet scenarios while keeping the baseline workbook unchanged.
//!
//! Input edits persist between requests; each edit clears calculated results.
//! Compiled formulas are retained. For dependency tracking and reuse of unaffected
//! results across edits, use [`Overlay`]. Work counters and failures accumulate
//! until a new context begins.
use super::continuation::Continuation;
use super::*;

/// Editable scenarios that calculate requested outputs over a baseline workbook.
///
/// Choose outputs with [`Self::cell`], [`Self::outputs`], or [`Self::all`]; formula
/// dependencies are calculated as needed. Each input edit clears calculated results.
/// Input edits and compiled formulas remain available between requests.
/// Work usage accumulates across requests and edits. An operational failure remains
/// active until [`Self::begin`] starts a new context and resets usage and results.
pub struct Evaluation<'a, W = &'a Workbook> {
    source: Source<'a, W>,
    context: EvaluationContext,
    state: Continuation,
    completed: crate::coordinate_hash::CellSet,
    spill_resets: std::collections::BTreeSet<Key>,
    failure: Option<EvaluationFailure>,
}
/// Configures the workbook, formula policies, and extensions for an evaluation.
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
        let meter = crate::budget::BudgetMeter::new(context.budgets)?
            .with_optional_limits(context.evaluation_limits());
        let state = Continuation::new(meter);
        Ok(Self {
            source,
            context,
            state,
            completed: Default::default(),
            spill_resets: Default::default(),
            failure: None,
        })
    }
    /// Select detailed accounting before any work in an ordinary calculation cycle.
    pub(super) fn set_profiling(&mut self, profiling: bool) {
        self.state.budget.profiling = profiling;
    }
    source_methods!();
    /// Discard pending authored edits and calculated results, keeping the baseline unchanged.
    pub fn discard_edits(&mut self) {
        self.reset();
    }
    /// Clear calculated results while retaining authored edits and compiled preparation.
    pub fn clear_calculated_cache(&mut self) {
        self.clear_cache();
    }

    fn policy_changed(&mut self, _previous: &OverlayPolicy) {
        self.clear_cache();
    }
    fn source_changed(&mut self, _target: EvaluationTarget) {
        self.clear_cache();
    }
    /// Discard calculated values and spill results.
    ///
    /// Input edits, work usage, and any previous failure remain. Use [`Self::begin`]
    /// to reset usage and recover from a failed calculation.
    pub fn clear_cache(&mut self) {
        self.state.clear_results();
        self.completed.clear();
    }
    /// Remove temporary inputs and calculated results, retaining compiled formulas.
    ///
    /// This does not reset work usage or a previous failure; use [`Self::begin`]
    /// for those.
    pub fn reset(&mut self) {
        self.source.inputs = crate::run_overlay::RunInputs::empty();
        self.clear_cache();
        // Cleanup belonged to the discarded scenario, not to the restored baseline.
        self.spill_resets.clear();
    }
    /// Start a new calculation context, clearing results, usage, and any failure.
    ///
    /// Temporary input edits and compiled formulas are retained.
    pub fn begin(&mut self, context: EvaluationContext) -> Result<(), OverlayError> {
        let meter = crate::budget::BudgetMeter::new(context.budgets)?
            .with_optional_limits(context.evaluation_limits());
        let state = Continuation::new(meter);
        self.context = context;
        self.state = state;
        self.completed.clear();
        self.failure = None;
        Ok(())
    }
    /// Start a new cycle, retaining edits and clearing results, work, and failure.
    pub fn start_cycle(&mut self, context: EvaluationContext) -> Result<(), OverlayError> {
        self.begin(context)
    }
    /// Select optional profiling while retaining this evaluation's prepared state.
    pub fn into_calculation(mut self, profiling: bool) -> Calculation<'a, W> {
        self.set_profiling(profiling);
        Calculation {
            inner: self,
            profiling,
        }
    }
    /// Read the time, randomness, locale, and limits used by this evaluation.
    pub fn context(&self) -> &EvaluationContext {
        &self.context
    }
    /// Calculate a cell and the dependencies needed for its value.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationFailure> {
        self.perform(|session| session.cell(target.sheet, target.address))
    }
    /// Calculate the requested cells in order, preserving duplicate requests.
    ///
    /// Returns an error rather than a partial result if any request fails.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.perform(|session| output::outputs(session, targets))
    }
    /// Calculate occupied cells and any spill cells produced by their formulas.
    pub fn all(&mut self) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.perform(output::all)
    }
    /// Memoized values from successful calls, including dependencies and spill children.
    /// Authored literals read directly by range consumers need no memo entry.
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
    /// Read a previously calculated result without running the formula again.
    pub fn cached_value(&self, target: EvaluationTarget) -> Option<&CellValue> {
        let key = (target.sheet, target.address);
        if !self.completed.contains(&key) {
            return None;
        }
        self.state.values().get(&key)
    }
    /// Inspect logical memo storage retained in this direct cycle, not allocator bytes.
    pub fn retained_usage(&self) -> crate::run::MemoRetentionUsage {
        self.state.memo_usage
    }
    /// Read work usage accumulated since construction or the last [`Self::begin`].
    pub fn usage(&self) -> &crate::RunUsage {
        self.state.usage()
    }
    /// Configure native persistence of authored edits and calculation preparation.
    pub fn save_snapshot(&self) -> crate::artifact::SnapshotSaveBuilder<'_> {
        crate::artifact::SnapshotSaveBuilder::checkpoint(self.snapshot())
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
    /// Return work usage and any failure recorded during this evaluation.
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
        // Admissions are journaled at the shared memo boundary, including spill
        // children. Failed calls retain their local memo but publish no changes.
        if let Some(admitted) = &mut session.admitted {
            if result.is_ok() {
                for key in admitted.drain(..) {
                    self.completed.insert(key);
                    // Scalar formulas also clear saved children from earlier spills.
                    if self
                        .source
                        .effective()
                        .cell(key.0, key.1)
                        .is_some_and(|cell| cell.formula.is_some())
                    {
                        self.spill_resets.insert(key);
                    }
                }
            } else {
                admitted.clear();
            }
        }
        drop(session);
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
    /// Build an ordinary calculation without detailed profiling. Quotas remain enforced.
    pub fn build_calculation(
        self,
        context: EvaluationContext,
    ) -> Result<super::Calculation<'a, W>, OverlayError> {
        self.build_calculation_with_profiling(context, false)
    }
    /// Build ordinary calculation with profiling independently selected from quotas.
    pub fn build_calculation_with_profiling(
        self,
        context: EvaluationContext,
        profiling: bool,
    ) -> Result<super::Calculation<'a, W>, OverlayError> {
        let mut inner = self.build(context)?;
        inner.set_profiling(profiling);
        Ok(super::Calculation { inner, profiling })
    }
    /// Validate the options and create an evaluation with the supplied context.
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
