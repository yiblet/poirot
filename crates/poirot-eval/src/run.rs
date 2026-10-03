//! One execution lifetime: context, budgets, dependency ports and memoized values.
//!
//! Use one [`EvaluationRun`] for several read-only targets so dependencies are
//! computed once in its memo. Compile again after formula/name/sheet topology changes;
//! a new run sees literal edits and starts with an empty memo. For reuse across
//! evaluations with edits, use the primary [`crate::overlay::Overlay`] API. For parallel what-if requests sharing one
//! immutable source and plan, use [`shared_workbook::SharedWorkbook`]; each run has
//! its own inputs, budgets, and memo.
//!
//! ```
//! use poirot_eval::{FormulaCompiler, EvaluationPolicy, WorkbookView, EvaluationRun, EvaluationTarget};
//! use poirot_model::{Workbook, CellValue};
//! let mut workbook = Workbook::new();
//! let sheet = workbook.add_sheet("Sheet1")?;
//! sheet.set_value("A1".parse()?, 3.0)?;
//! sheet.set_formula("B1".parse()?, "A1*2", CellValue::Blank)?;
//! sheet.set_formula("C1".parse()?, "B1+1", CellValue::Blank)?;
//! sheet.set_formula("D1".parse()?, "B1+2", CellValue::Blank)?;
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

/// Cell result reuse and completion observation ports.
pub use completion::{
    CachedCellReader, CellCompletion, CellObserver, CellProvenance, same_result_identity,
};
/// Actual consumed-read events used to construct dependency certificates.
pub use consumption::{ContextRead, DependencyRead, DependencyReadSink, ReadEvent};

/// Execution counters for one run, distinguishing formula execution, prepared lookups,
/// session memo hits, and validated cross-run cache hits.
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
            cells: 100_000,
            bytes: 64 * 1024 * 1024,
        }
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
/// Observation of builtin handlers actually entered during execution.
pub use crate::consumed_calls::{BuiltinInvocationObserver, ConsumedBuiltinInvocation};

/// Shared-session probes reuse only this run's memo. Construct another run to read
/// changed values/context; no result is carried between run lifetimes.
pub struct EvaluationRun<'a> {
    /// Execution state shared by this run's probes and nested formula calls.
    pub(super) session: Session<'a>,
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
        Ok(Self { session })
    }
    /// Start a run using explicit formatting and computation policies.
    pub fn new_with_configuration(
        view: WorkbookView<'a>,
        configuration: &'a crate::RunConfiguration,
        budgets: EvaluationBudgets,
    ) -> Result<Self, EvaluationFailure> {
        configuration.validate()?;
        let mut run = Self::new(view, configuration.context(), budgets)?;
        run.session.currency = configuration.formatting().currency();
        run.session.text_policy = configuration.text_policy();
        run.session.numeric_policy = configuration.numeric_policy();
        run.session.builtin_policy = configuration.builtin_policy();
        run.session.array_result_policy = configuration.array_result_policy();
        Ok(run)
    }
    /// Start a configured run with its exact extension registry.
    pub fn new_with_configuration_and_extensions(
        view: WorkbookView<'a>,
        configuration: &'a crate::RunConfiguration,
        budgets: EvaluationBudgets,
        registry: &'a crate::extensions::ExtensionRegistry,
    ) -> Result<Self, EvaluationFailure> {
        configuration.validate()?;
        let mut run = Self::new_with_extensions(view, configuration.context(), budgets, registry)?;
        run.session.currency = configuration.formatting().currency();
        run.session.text_policy = configuration.text_policy();
        run.session.numeric_policy = configuration.numeric_policy();
        run.session.builtin_policy = configuration.builtin_policy();
        run.session.array_result_policy = configuration.array_result_policy();
        Ok(run)
    }
    /// Set before evaluating. Retention exhaustion aborts before cloning the
    /// rejected memo value; previously completed memo entries remain available.
    pub fn with_memo_limits(mut self, limits: MemoRetentionLimits) -> Self {
        self.session.memo_limits = Some(limits);
        self
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
        &self.session.run_usage
    }
    /// Evaluate a stored cell using this run's shared dependency memo.
    pub fn evaluate_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<CellValue, EvaluationFailure> {
        self.session.cell(target.sheet, target.address)
    }
    /// Evaluate a supplied prepared formula at the given coordinates.
    pub fn evaluate_formula(
        &mut self,
        target: EvaluationTarget,
        formula: &PreparedFormula,
    ) -> Result<CellValue, EvaluationFailure> {
        self.session.prepared_at(target, formula)
    }
    /// Evaluate supplied bound syntax at the given coordinates.
    pub fn evaluate_bound(
        &mut self,
        target: EvaluationTarget,
        formula: &bound::FormulaPlan,
    ) -> Result<CellValue, EvaluationFailure> {
        self.session.bound_at(target, formula)
    }
    /// Evaluate ordered probes and finish with their values and usage report.
    pub fn evaluate_probes(
        mut self,
        probes: &[EvaluationProbe<'_>],
    ) -> RunReport<Vec<CellEvaluation>> {
        let result = (|| {
            self.session.budget.allocation(probes.len())?;
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
                values.push(CellEvaluation { target, value });
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
        self.session.budget.allocation(targets.len())?;
        for target in targets {
            self.evaluate_cell(*target)?;
        }
        Ok(())
    }
    /// Read-only full recalculation with the ordinary discovery/usage gates.
    /// Inputs suppress formulas and newly stored cells count in the sparse scan.
    pub fn evaluate_workbook(&mut self) -> Result<usize, EvaluationFailure> {
        let effective =
            crate::run_overlay::EffectiveWorkbook::new(self.session.workbook, self.session.inputs);
        let mut targets = Vec::new();
        for sheet in 0..self.session.workbook.sheets().len() {
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
            self.session
                .budget
                .charge(crate::WorkKind::SparseScan, count as u64)?;
            for (address, cell) in effective
                .stored(area)
                .map_err(|_| EvaluationFailure::StalePlan)?
            {
                if cell.formula.is_some() {
                    self.session.budget.check_allocation(targets.len() + 1)?;
                    self.session
                        .budget
                        .charge(crate::WorkKind::Materialization, 1)?;
                    targets.push(EvaluationTarget {
                        sheet,
                        address: *address,
                    });
                }
            }
        }
        self.session.budget.allocation(targets.len())?;
        for target in &targets {
            self.evaluate_cell(*target)?;
        }
        Ok(targets.len())
    }
    /// Finish while moving the memo into the report's retained result owner.
    pub fn finish_retaining_cells<T>(
        mut self,
        result: Result<T, EvaluationFailure>,
    ) -> RetainedRun<T> {
        let cells = std::mem::take(&mut self.session.cache);
        let memo_usage = self.session.memo_usage;
        RetainedRun {
            memo_usage,
            report: self.finish(result),
            cells,
        }
    }
    /// Finish with the caller's result and accumulated accounting.
    pub fn finish<T>(self, result: Result<T, EvaluationFailure>) -> RunReport<T> {
        RunReport {
            execution: self.session.run_usage,
            outcome: self.session.budget.outcome(result),
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
