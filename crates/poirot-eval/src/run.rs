//! One execution lifetime: context, budgets, dependency ports and memoized values.
//!
//! Use one [`EvaluationRun`] for several read-only targets so dependencies are
//! computed once in its memo. Compile again after formula/name/sheet topology changes;
//! a new run sees literal edits and starts with an empty memo. For reuse across
//! evaluations with edits, use the primary [`crate::overlay::Overlay`] API. For parallel what-if requests sharing one
//! immutable source and plan, use [`shared_workbook::SharedWorkbook`]; each run has
//! its own inputs, budgets, and memo.
//!
//! Repeated SUMIF and conformable SUMPRODUCT calls may share one bounded scalar
//! result within a run after each call prepares its own arguments. The first
//! repetition admits reuse; subsequent identical resolved calls skip the ordered
//! range scan. Edits and new evaluations discard this slot. External execution
//! observers retain ordinary reads. Work counters measure kernels actually entered.
//!
//! ```
//! use poirot_eval::{FormulaCompiler, EvaluationPolicy, WorkbookView, EvaluationRun, EvaluationTarget};
//! use poirot_model::{Workbook, CellValue};
//! let mut workbook = Workbook::new();
//! let sheet = workbook.add_sheet("Sheet1")?;
//! sheet.set_value("A1".parse()?, 3.0)?;
//! sheet.set_formula("B1".parse()?, "A1*2")?;
//! sheet.set_formula("C1".parse()?, "B1+1")?;
//! sheet.set_formula("D1".parse()?, "B1+2")?;
//! let policy = EvaluationPolicy::default();
//! let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
//! let view = WorkbookView::new(&workbook, &plan, &policy)?;
//! let mut run = EvaluationRun::new(view, Default::default(), Default::default())?;
//! let c = run.evaluate_cell(EvaluationTarget { sheet: 0, address: "C1".parse()? })?;
//! let d = run.evaluate_cell(EvaluationTarget { sheet: 0, address: "D1".parse()? })?;
//! assert_eq!((c, d), (CellValue::Number(7.0), CellValue::Number(8.0)));
//! assert_eq!(run.usage().formula_executions, 3); // B1 is evaluated once.
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::{
    CellEvaluation, EvaluationBudgets, EvaluationFailure, EvaluationInputs, EvaluationOutcome,
    EvaluationTarget, Session, bound,
    budget::BudgetMeter,
    compiler::{CompiledWorkbook, EvaluationPolicy, PreparedFormula},
};
#[cfg(test)]
use poirot_model::CellAddress;
use poirot_model::{CellValue, Workbook};

mod completion;
mod consumption;
// The immutable source shell is a child so its checked source can build a view
// without exposing an unchecked constructor to other evaluation callers.
#[path = "shared_workbook.rs"]
/// Immutable workbook ownership for independent what-if runs.
pub mod shared_workbook;

// Cell result reuse and completion observation ports.
pub use completion::{
    CachedCellReader, CellCompletion, CellObserver, CellProvenance, same_result_identity,
};
// Actual consumed-read events used to construct dependency certificates.
pub use consumption::{ContextRead, DependencyRead, DependencyReadSink, ReadEvent};

/// Execution counters for one run, distinguishing formula execution, prepared lookups,
/// session memo hits, and validated cross-run cache hits.
/// Counters saturate at `u64::MAX`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunUsage {
    /// Runtime parsing of computed reference text; excluded from pure preparation counters.
    pub dynamic_parse_attempts: u64,
    /// Formula cell bodies executed.
    pub formula_executions: u64,
    /// Defined-name formula bodies executed.
    pub named_formula_executions: u64,
    /// Prepared formula lookups performed.
    pub compiled_lookups: u64,
    /// Reads satisfied by this run's memo.
    pub session_memo_hits: u64,
    /// Exact lookup selections satisfied without scanning candidate cells.
    pub lookup_prefix_hits: u64,
    /// Candidate cells scanned while extending eligible exact lookup indexes.
    pub lookup_prefix_scans: u64,
    /// Reads satisfied by a validated cross-run cache.
    pub external_cache_hits: u64,
}
/// A calculation outcome and its execution counters.
/// Inspect `outcome` for operational failure; usage alone does not establish success.
#[derive(Clone, Debug)]
pub struct RunReport<T> {
    /// Caller result or operational abort, including metered usage.
    pub outcome: EvaluationOutcome<T>,
    /// Execution and reuse counters for this run.
    pub execution: RunUsage,
}
/// A checked immutable read view; ordinary value edits are read afresh on each run.
pub struct WorkbookView<'a> {
    workbook: &'a Workbook,
    plan: &'a CompiledWorkbook,
}
impl<'a> WorkbookView<'a> {
    /// Validate that the prepared plan still matches workbook topology and policy.
    pub fn new(
        workbook: &'a Workbook,
        plan: &'a CompiledWorkbook,
        policy: &EvaluationPolicy,
    ) -> Result<Self, EvaluationFailure> {
        if !plan.matches(workbook, policy) {
            return Err(EvaluationFailure::StalePlan);
        }
        Ok(Self { workbook, plan })
    }
}
/// Logical bounds on owned Session memo retention, independent of calculation budgets.
/// Each accepted entry costs 128 fixed units plus its owned text storage bytes.
/// These are deterministic retention units, not an allocator-exact heap measure.
/// Defaults are unlimited; `usize::MAX` disables the respective policy limit.
/// Intrinsic size overflow remains an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoRetentionLimits {
    /// Maximum retained memo entries.
    pub cells: usize,
    /// Maximum retained logical storage bytes.
    pub bytes: usize,
}
impl Default for MemoRetentionLimits {
    fn default() -> Self {
        Self {
            cells: usize::MAX,
            bytes: usize::MAX,
        }
    }
}
impl MemoRetentionLimits {
    /// Set maximum cached entries across the retained state's lifetime; zero admits none.
    /// Admissions are checked before publication; tightening below usage explicitly refuses.
    pub fn max_cached_cells(mut self, maximum: usize) -> Self {
        self.cells = maximum;
        self
    }
    /// Remove the cached entries quota without clearing retained state.
    pub fn without_cached_cells_limit(mut self) -> Self {
        self.cells = usize::MAX;
        self
    }
    /// Inspect the cached entries quota; `None` means no configured maximum.
    pub fn cached_cells_limit(&self) -> Option<usize> {
        (self.cells != usize::MAX).then_some(self.cells)
    }
    /// Set maximum logical retention bytes (128 fixed units per entry plus owned text) across the retained state's lifetime; zero admits none.
    /// Admissions are checked before publication; tightening below usage explicitly refuses.
    pub fn max_logical_memo_bytes(mut self, maximum: usize) -> Self {
        self.bytes = maximum;
        self
    }
    /// Remove the logical retention bytes (128 fixed units per entry plus owned text) quota without clearing retained state.
    pub fn without_logical_memo_bytes_limit(mut self) -> Self {
        self.bytes = usize::MAX;
        self
    }
    /// Inspect the logical retention bytes (128 fixed units per entry plus owned text) quota; `None` means no configured maximum.
    pub fn logical_memo_bytes_limit(&self) -> Option<usize> {
        (self.bytes != usize::MAX).then_some(self.bytes)
    }
}
/// Logical memo entries and retained bytes, using the same units as [`MemoRetentionLimits`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoRetentionUsage {
    /// Number of retained memo entries.
    pub cells: usize,
    /// Retained logical storage bytes.
    pub bytes: usize,
}
/// Coordinate-keyed memo moved intact from a completed or aborted evaluation.
///
/// This uses a randomized noncryptographic hasher for validated numeric keys.
/// Keys, values and map operations match `HashMap`; explicitly typed consumers
/// should use this alias rather than a `HashMap` with the default hasher.
pub type CellMemo = std::collections::HashMap<
    (usize, poirot_model::CellAddress),
    CellValue,
    foldhash::fast::RandomState,
>;
/// Owned results of one run. This is the Session memo moved at completion,
/// never a second cache and never a source of defaults for another run.
pub struct RetainedRun<T> {
    /// Completion or abort report from the finished run.
    pub report: RunReport<T>,
    /// Storage charged for the retained memo.
    pub memo_usage: MemoRetentionUsage,
    /// Memo moved from the run without duplicating its values.
    pub cells: CellMemo,
}
/// A requested cell, prepared formula, or bound expression evaluated at a target cell.
/// Several probes in one [`EvaluationRun`] share that run's memoized dependencies.
pub enum EvaluationProbe<'a> {
    /// Evaluate the workbook cell at the requested target.
    Cell(EvaluationTarget),
    /// Execute a supplied prepared formula at the requested target.
    Formula {
        /// Coordinates used for relative evaluation and attribution.
        target: EvaluationTarget,
        /// Prepared formula independent of the stored cell source.
        formula: &'a PreparedFormula,
    },
    /// Execute supplied bound syntax at the requested target.
    Bound {
        /// Coordinates used for relative evaluation and attribution.
        target: EvaluationTarget,
        /// Bound syntax to execute.
        formula: &'a bound::FormulaPlan,
    },
}
// Observation of builtin handlers actually entered during execution.
pub use crate::consumed_calls::{BuiltinInvocationObserver, ConsumedBuiltinInvocation};

/// Shared-session probes reuse only this run's memo. Construct another run to read
/// changed values/context; no result is carried between run lifetimes.
pub struct EvaluationRun<'a> {
    /// Execution state shared by this run's probes and nested formula calls.
    pub(super) session: Session<'a>,
    pub(super) failure: Option<EvaluationFailure>,
}
impl<'a> EvaluationRun<'a> {
    /// Start a bounded run over a checked workbook view without extensions.
    pub fn new(
        view: WorkbookView<'a>,
        context: EvaluationInputs,
        budgets: EvaluationBudgets,
    ) -> Result<Self, EvaluationFailure> {
        Self::new_with_registry(view, context, budgets, None)
    }
    /// Start a run with the registry required by the checked plan.
    pub fn new_with_extensions(
        view: WorkbookView<'a>,
        context: EvaluationInputs,
        budgets: EvaluationBudgets,
        registry: &'a crate::extensions::ExtensionRegistry,
    ) -> Result<Self, EvaluationFailure> {
        Self::new_with_registry(view, context, budgets, Some(registry))
    }
    fn new_with_registry(
        view: WorkbookView<'a>,
        context: EvaluationInputs,
        budgets: EvaluationBudgets,
        registry: Option<&'a crate::extensions::ExtensionRegistry>,
    ) -> Result<Self, EvaluationFailure> {
        let meter = BudgetMeter::new(budgets)?;
        if view.plan.policy().registry.extensions
            != registry.map(crate::extensions::ExtensionRegistry::identity)
        {
            return Err(EvaluationFailure::StalePlan);
        }
        let mut session = Session::new(view.workbook, context, meter);
        session.plan = Some(view.plan);
        session.extensions = registry;
        Ok(Self {
            session,
            failure: None,
        })
    }
    /// Start a run using explicit formatting and computation policies.
    pub fn new_with_configuration(
        view: WorkbookView<'a>,
        configuration: &'a crate::RunConfiguration,
        budgets: EvaluationBudgets,
    ) -> Result<Self, EvaluationFailure> {
        let mut run = Self::new(view, configuration.context(), budgets)?;
        run.session.currency = configuration.formatting().currency();
        run.session.number_locale = configuration.number_locale();
        run.session.text_policy = configuration.text_policy();

        run.session.builtin_policy = configuration.builtin_policy();
        run.session.array_result_policy = configuration.array_result_policy();
        run.session.producer_policy = configuration.producer_policy();
        Ok(run)
    }
    /// Start a configured run with its exact extension registry.
    pub fn new_with_configuration_and_extensions(
        view: WorkbookView<'a>,
        configuration: &'a crate::RunConfiguration,
        budgets: EvaluationBudgets,
        registry: &'a crate::extensions::ExtensionRegistry,
    ) -> Result<Self, EvaluationFailure> {
        let mut run = Self::new_with_extensions(view, configuration.context(), budgets, registry)?;
        run.session.currency = configuration.formatting().currency();
        run.session.number_locale = configuration.number_locale();
        run.session.text_policy = configuration.text_policy();

        run.session.builtin_policy = configuration.builtin_policy();
        run.session.array_result_policy = configuration.array_result_policy();
        run.session.producer_policy = configuration.producer_policy();
        Ok(run)
    }
    /// Set before evaluating. Retention exhaustion aborts before cloning the
    /// rejected memo value; previously completed memo entries remain available.
    pub fn with_memo_limits(mut self, limits: MemoRetentionLimits) -> Self {
        if self.session.runtime.memo_usage.cells > limits.cells
            || self.session.runtime.memo_usage.bytes > limits.bytes
        {
            self.failure = Some(EvaluationFailure::MemoRetentionLimit {
                limits,
                consumed: self.session.runtime.memo_usage,
                requested: Default::default(),
                source: self.session.runtime.consumer,
            });
        } else {
            self.session.memo_limits = Some(limits);
        }
        self
    }
    /// Set memo retention before work, refusing a tighter quota below already retained usage.
    pub fn try_with_memo_limits(
        self,
        limits: MemoRetentionLimits,
    ) -> Result<Self, EvaluationFailure> {
        let run = self.with_memo_limits(limits);
        if let Some(failure) = &run.failure {
            return Err(failure.clone());
        }
        Ok(run)
    }
    /// Assignments suppress baseline formulas, without mutating the checked plan source.
    pub(crate) fn with_inputs(mut self, inputs: &'a crate::run_overlay::RunInputs) -> Self {
        self.session.inputs = Some(inputs);
        self
    }
    /// Record reads actually consumed by this execution.
    pub fn with_dependency_sink(mut self, sink: &'a mut dyn DependencyReadSink) -> Self {
        self.session.dependency_sink = Some(sink);
        self
    }
    /// Install an external cache port that validates entries before reuse.
    pub fn with_cell_reader(mut self, reader: &'a mut dyn CachedCellReader) -> Self {
        self.session.cell_reader = Some(reader);
        self
    }
    /// Observe completed cells, including their result provenance.
    pub fn with_cell_observer(mut self, observer: &'a mut dyn CellObserver) -> Self {
        self.session.cell_observer = Some(observer);
        self
    }
    /// Observe actual builtin handler entry. Failure aborts before handler effects;
    /// previously accepted entries belong to the caller and survive that abort.
    pub fn with_builtin_invocation_observer(
        mut self,
        observer: &'a mut dyn BuiltinInvocationObserver,
    ) -> Self {
        self.session.builtin_invocation_observer = Some(observer);
        self
    }
    /// Borrowed actual execution events. The observer admits retention before copying;
    /// callback failure is operational and cannot be caught by spreadsheet handlers.
    pub fn with_execution_observer(
        mut self,
        observer: &'a mut dyn crate::execution_observation::ExecutionObserver,
    ) -> Self {
        self.session.execution_observer = Some(observer);
        self
    }
    /// Return the active text source semantics.
    pub fn text_policy(&self) -> poirot_text::TextSemanticsPolicy {
        self.session.text_policy
    }
    /// Borrow cumulative execution and reuse counters.
    pub fn usage(&self) -> &RunUsage {
        &self.session.runtime.run_usage
    }
    /// Evaluate a stored cell using this run's shared dependency memo.
    pub fn evaluate_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<CellValue, EvaluationFailure> {
        self.perform(|session| session.cell(target.sheet, target.address))
    }
    /// Evaluate a supplied prepared expression at the given coordinates.
    /// This does not publish a worksheet cell; blank expression results remain blank.
    pub fn evaluate_formula(
        &mut self,
        target: EvaluationTarget,
        formula: &PreparedFormula,
    ) -> Result<CellValue, EvaluationFailure> {
        self.perform(|session| session.prepared_at(target, formula))
    }
    /// Evaluate supplied bound syntax at the given coordinates without worksheet publication.
    pub fn evaluate_bound(
        &mut self,
        target: EvaluationTarget,
        formula: &bound::FormulaPlan,
    ) -> Result<CellValue, EvaluationFailure> {
        self.perform(|session| session.bound_at(target, formula))
    }
    fn perform<T>(
        &mut self,
        execute: impl FnOnce(&mut Session<'a>) -> Result<T, EvaluationFailure>,
    ) -> Result<T, EvaluationFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let result = execute(&mut self.session);
        if let Err(failure) = &result {
            self.failure = Some(failure.clone());
        }
        result
    }
    /// Evaluate ordered probes and finish with their values and usage report.
    pub fn evaluate_probes(
        mut self,
        probes: &[EvaluationProbe<'_>],
    ) -> RunReport<Vec<CellEvaluation>> {
        let result = (|| {
            self.session.runtime.budget.allocation(probes.len())?;
            let mut values = Vec::with_capacity(probes.len());
            for probe in probes {
                let (target, value) = match probe {
                    EvaluationProbe::Cell(target) => (*target, self.evaluate_cell(*target)?),
                    EvaluationProbe::Formula { target, formula } => {
                        (*target, self.evaluate_formula(*target, formula)?)
                    }
                    EvaluationProbe::Bound { target, formula } => {
                        (*target, self.evaluate_bound(*target, formula)?)
                    }
                };
                values.push(self.session.output(target, value));
            }
            Ok(values)
        })();
        self.finish(result)
    }
    /// Same target transport admission as evaluate_probes, retaining only the
    /// Session memo for the caller's later output projection.
    pub fn evaluate_targets(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<(), EvaluationFailure> {
        self.perform(|session| {
            session.runtime.budget.allocation(targets.len())?;
            for target in targets {
                session.cell(target.sheet, target.address)?;
            }
            Ok(())
        })
    }
    /// Read-only full recalculation with the ordinary discovery/usage gates.
    /// Inputs suppress formulas and newly stored cells count in the sparse scan.
    pub fn evaluate_workbook(&mut self) -> Result<usize, EvaluationFailure> {
        self.perform(|session| {
            let effective =
                crate::run_overlay::EffectiveWorkbook::new(session.workbook, session.inputs);
            let mut targets = Vec::new();
            for sheet in 0..session.workbook.sheets().len() {
                let area = poirot_model::query::RangeMembership::new(
                    sheet,
                    poirot_model::CellAddress { row: 0, col: 0 },
                    poirot_model::CellAddress {
                        row: poirot_model::MAX_ROWS - 1,
                        col: poirot_model::MAX_COLUMNS - 1,
                    },
                )
                .map_err(|_| EvaluationFailure::StalePlan)?;
                let count = effective
                    .stored(area)
                    .map_err(|_| EvaluationFailure::StalePlan)?
                    .count();
                session
                    .runtime
                    .budget
                    .charge(crate::WorkKind::SparseScan, count as u64)?;
                for (address, cell) in effective
                    .stored(area)
                    .map_err(|_| EvaluationFailure::StalePlan)?
                {
                    if cell.formula.is_some() {
                        session.runtime.budget.check_allocation(targets.len() + 1)?;
                        session
                            .runtime
                            .budget
                            .charge(crate::WorkKind::Materialization, 1)?;
                        targets.push(EvaluationTarget {
                            sheet,
                            address: *address,
                        });
                    }
                }
            }
            session.runtime.budget.allocation(targets.len())?;
            for target in &targets {
                session.cell(target.sheet, target.address)?;
            }
            Ok(targets.len())
        })
    }
    /// Finish while moving the memo into the report's retained result owner.
    pub fn finish_retaining_cells<T>(
        mut self,
        result: Result<T, EvaluationFailure>,
    ) -> RetainedRun<T> {
        let cells = std::mem::take(&mut self.session.runtime.cache);
        let memo_usage = self.session.runtime.memo_usage;
        RetainedRun {
            memo_usage,
            report: self.finish(result),
            cells,
        }
    }
    /// Finish with the caller's result and accumulated accounting.
    pub fn finish<T>(mut self, result: Result<T, EvaluationFailure>) -> RunReport<T> {
        RunReport {
            execution: std::mem::take(&mut self.session.runtime.run_usage),
            outcome: self
                .session
                .runtime
                .budget
                .take()
                .outcome(self.failure.map_or(result, Err)),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod memo_retention_tests;

/// Borrowed ports for one actual Session; no second evaluation is performed.
pub struct RunObservers<'a> {
    /// Sink for consumed dependency reads.
    pub reads: &'a mut dyn DependencyReadSink,
    /// Sink for completed cell values and provenance.
    pub cells: &'a mut dyn CellObserver,
    /// Sink for actual builtin invocations.
    pub builtins: &'a mut dyn crate::BuiltinInvocationObserver,
    /// Sink for execution operations and reuse events.
    pub operations: &'a mut dyn crate::execution_observation::ExecutionObserver,
}
