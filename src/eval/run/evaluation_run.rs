use super::evaluation_probe::EvaluationProbe;
use super::retained_run::RetainedRun;
use super::run_report::RunReport;
use super::workbook_view::WorkbookView;
use crate::eval::adapters::{CallContext, FunctionAdapter, builtins};
use crate::eval::compile::bound;
use crate::eval::compile::bytecode::Compiled;
use crate::eval::compile::compiler::PreparedFormula;
use crate::eval::config::EvaluationInputs;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::RunUsage;
use crate::eval::limits::{BudgetMeter, EvaluationBudgets, EvaluationError, MemoRetentionLimits};
use crate::eval::observe::BuiltinInvocationObserver;
use crate::eval::ports::DependencyReadSink;
use crate::eval::ports::{CachedCellReader, CellObserver};
use crate::eval::session::{
    CellEvaluator, FormulaExecution, ResultProjection, RunPolicies, Session,
};
use crate::eval::values::CellEvaluation;
use crate::model::CellValue;
use poirot_functions::scalar::ParamVec;
use poirot_parsing::formula::FunctionName;

/// Calculates several requested cells or formulas with shared budgets and memoized dependencies.
/// Reuse lasts for this run; construct a new run to observe changed source values
/// or execution settings.
pub struct EvaluationRun<'a> {
    /// Execution state shared by this run's probes and nested formula calls.
    pub(in crate::eval) session: Session<'a>,
    pub(in crate::eval) failure: Option<EvaluationError>,
}

impl<'a> EvaluationRun<'a> {
    /// Start a bounded run over a checked workbook view without extensions.
    pub fn new(
        view: WorkbookView<'a>,
        context: EvaluationInputs,
        budgets: EvaluationBudgets,
    ) -> Result<Self, EvaluationError> {
        Self::new_with_registry(view, RunPolicies::from_inputs(context), budgets, None)
    }

    /// Start a run with the registry required by the checked plan.
    pub fn new_with_extensions(
        view: WorkbookView<'a>,
        context: EvaluationInputs,
        budgets: EvaluationBudgets,
        registry: &'a crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Result<Self, EvaluationError> {
        Self::new_with_registry(
            view,
            RunPolicies::from_inputs(context),
            budgets,
            Some(registry),
        )
    }

    pub(super) fn new_with_registry(
        view: WorkbookView<'a>,
        configuration: RunPolicies<'a>,
        budgets: EvaluationBudgets,
        registry: Option<&'a crate::eval::functions::extensions::ExtensionRegistry>,
    ) -> Result<Self, EvaluationError> {
        let meter = BudgetMeter::new(budgets)?;
        if view.plan.policy().registry.extensions.as_deref()
            != registry
                .map(crate::eval::functions::extensions::ExtensionRegistry::identity)
                .as_ref()
        {
            return Err(EvaluationError::StalePlan);
        }
        let mut session = Session::with_configuration(view.workbook, configuration, meter);
        session.sources.plan = Some(view.plan);
        session.sources.extensions = registry;
        Ok(Self {
            session,
            failure: None,
        })
    }

    /// Start a run using explicit formatting and computation policies.
    pub fn new_with_configuration(
        view: WorkbookView<'a>,
        configuration: &'a crate::eval::config::RunConfiguration,
        budgets: EvaluationBudgets,
    ) -> Result<Self, EvaluationError> {
        Self::new_with_registry(
            view,
            RunPolicies::from_run_configuration(configuration),
            budgets,
            None,
        )
    }

    /// Start a configured run with its exact extension registry.
    pub fn new_with_configuration_and_extensions(
        view: WorkbookView<'a>,
        configuration: &'a crate::eval::config::RunConfiguration,
        budgets: EvaluationBudgets,
        registry: &'a crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Result<Self, EvaluationError> {
        Self::new_with_registry(
            view,
            RunPolicies::from_run_configuration(configuration),
            budgets,
            Some(registry),
        )
    }

    /// Set before evaluating. Retention exhaustion aborts before cloning the
    /// rejected memo value; previously completed memo entries remain available.
    pub fn with_memo_limits(mut self, limits: MemoRetentionLimits) -> Self {
        if self.session.state.runtime.memo.usage.cells > limits.cells
            || self.session.state.runtime.memo.usage.bytes > limits.bytes
        {
            self.failure = Some(EvaluationError::MemoRetentionLimit {
                limits,
                consumed: self.session.state.runtime.memo.usage,
                requested: Default::default(),
                target: self.session.state.runtime.stack.consumer,
            });
        } else {
            self.session.state.memo_limits = Some(limits);
        }
        self
    }

    /// Set memo retention before work, refusing a tighter quota below already retained usage.
    pub fn try_with_memo_limits(
        self,
        limits: MemoRetentionLimits,
    ) -> Result<Self, EvaluationError> {
        let run = self.with_memo_limits(limits);
        if let Some(failure) = &run.failure {
            return Err(failure.clone());
        }
        Ok(run)
    }

    /// Assignments suppress baseline formulas, without mutating the checked plan source.
    pub(in crate::eval) fn with_inputs(
        mut self,
        inputs: &'a crate::eval::inputs::RunInputs,
    ) -> Self {
        self.session.sources.inputs = Some(inputs);
        self
    }

    /// Record reads actually consumed by this execution.
    pub fn with_dependency_sink(mut self, sink: &'a mut dyn DependencyReadSink) -> Self {
        self.session.ports.dependency_sink = Some(sink);
        self
    }

    /// Install an external cache port that validates entries before reuse.
    pub fn with_cell_reader(mut self, reader: &'a mut dyn CachedCellReader) -> Self {
        self.session.ports.cell_reader = Some(reader);
        self
    }

    /// Observe completed cells, including their result provenance.
    pub fn with_cell_observer(mut self, observer: &'a mut dyn CellObserver) -> Self {
        self.session.ports.cell_observer = Some(observer);
        self
    }

    /// Observe actual builtin handler entry. Failure aborts before handler effects;
    /// previously accepted entries belong to the caller and survive that abort.
    pub fn with_builtin_invocation_observer(
        mut self,
        observer: &'a mut dyn BuiltinInvocationObserver,
    ) -> Self {
        self.session.ports.builtin_invocation_observer = Some(observer);
        self
    }

    /// Borrowed actual execution events. The observer admits retention before copying;
    /// callback failure is operational and cannot be caught by spreadsheet handlers.
    pub fn with_execution_observer(
        mut self,
        observer: &'a mut dyn crate::eval::observe::ExecutionObserver,
    ) -> Self {
        self.session.ports.execution_observer = Some(observer);
        self
    }

    /// Borrow cumulative execution and reuse counters.
    pub fn usage(&self) -> &RunUsage {
        &self.session.state.runtime.run_usage
    }

    /// Evaluate a stored cell using this run's shared dependency memo.
    pub fn evaluate_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<CellValue, EvaluationError> {
        self.perform(|session| CellEvaluator::new(session).cell(target.sheet, target.address))
    }

    /// Evaluate a supplied prepared expression at the given coordinates.
    /// This does not publish a worksheet cell; blank expression results remain blank.
    pub fn evaluate_formula(
        &mut self,
        target: EvaluationTarget,
        formula: &PreparedFormula,
    ) -> Result<CellValue, EvaluationError> {
        self.perform(|session| FormulaExecution::new(session).prepared_at(target, formula))
    }

    /// Evaluate supplied bound syntax at the given coordinates without worksheet publication.
    pub fn evaluate_bound(
        &mut self,
        target: EvaluationTarget,
        formula: &bound::FormulaPlan,
    ) -> Result<CellValue, EvaluationError> {
        self.perform(|session| FormulaExecution::new(session).bound_at(target, formula))
    }

    pub(super) fn perform<T>(
        &mut self,
        execute: impl FnOnce(&mut Session<'a>) -> Result<T, EvaluationError>,
    ) -> Result<T, EvaluationError> {
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
            self.session.state.runtime.budget.allocation(probes.len())?;
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
                values.push(self.session.sources.output(target, value));
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
    ) -> Result<(), EvaluationError> {
        self.perform(|session| {
            session.state.runtime.budget.allocation(targets.len())?;
            for target in targets {
                CellEvaluator::new(session).cell(target.sheet, target.address)?;
            }
            Ok(())
        })
    }

    /// Read-only full recalculation with the ordinary discovery/usage gates.
    /// Inputs suppress formulas and newly stored cells count in the sparse scan.
    pub fn evaluate_workbook(&mut self) -> Result<usize, EvaluationError> {
        self.perform(|session| {
            let effective = crate::eval::inputs::EffectiveWorkbook::new(
                session.sources.workbook,
                session.sources.inputs,
            );
            let mut targets = Vec::new();
            for sheet in 0..session.sources.workbook.sheets().len() {
                let area = crate::model::query::RangeMembership::new(
                    sheet,
                    crate::model::CellAddress { row: 0, col: 0 },
                    crate::model::CellAddress {
                        row: crate::model::MAX_ROWS - 1,
                        col: crate::model::MAX_COLUMNS - 1,
                    },
                )
                .map_err(|_| EvaluationError::StalePlan)?;
                let count = effective
                    .stored(area)
                    .map_err(|_| EvaluationError::StalePlan)?
                    .count();
                session
                    .state
                    .runtime
                    .budget
                    .charge(crate::eval::limits::WorkKind::SparseScan, count as u64)?;
                for (address, cell) in effective
                    .stored(area)
                    .map_err(|_| EvaluationError::StalePlan)?
                {
                    if cell.formula.is_some() {
                        session
                            .state
                            .runtime
                            .budget
                            .check_allocation(targets.len() + 1)?;
                        session
                            .state
                            .runtime
                            .budget
                            .charge(crate::eval::limits::WorkKind::Materialization, 1)?;
                        targets.push(EvaluationTarget {
                            sheet,
                            address: *address,
                        });
                    }
                }
            }
            session.state.runtime.budget.allocation(targets.len())?;
            for target in &targets {
                CellEvaluator::new(session).cell(target.sheet, target.address)?;
            }
            Ok(targets.len())
        })
    }

    /// Finish while moving the memo into the report's retained result owner.
    pub fn finish_retaining_cells<T>(
        mut self,
        result: Result<T, EvaluationError>,
    ) -> RetainedRun<T> {
        let cells = std::mem::take(&mut self.session.state.runtime.memo.cache);
        let memo_usage = self.session.state.runtime.memo.usage;
        RetainedRun {
            memo_usage,
            report: self.finish(result),
            cells,
        }
    }

    /// Finish with the caller's result and accumulated accounting.
    pub fn finish<T>(mut self, result: Result<T, EvaluationError>) -> RunReport<T> {
        RunReport {
            execution: std::mem::take(&mut self.session.state.runtime.run_usage),
            outcome: self
                .session
                .state
                .runtime
                .budget
                .take()
                .outcome(self.failure.map_or(result, Err)),
        }
    }

    /// Call a builtin by name with a small, deferred argument vector.
    /// Parse individual expressions with poirot_parsing::formula and compile them with bytecode::compile. Unknown names return the spreadsheet #NAME? error.
    pub fn call(
        &mut self,
        target: EvaluationTarget,
        name: &FunctionName,
        parameters: ParamVec<Compiled>,
    ) -> Result<CellValue, EvaluationError> {
        self.call_with_adapter(&builtins(), target, name, parameters)
    }

    /// Evaluate through one family or a user-composed chain of family adapters.
    pub fn call_with_adapter(
        &mut self,
        adapter: &impl FunctionAdapter,
        target: EvaluationTarget,
        name: &FunctionName,
        parameters: ParamVec<Compiled>,
    ) -> Result<CellValue, EvaluationError> {
        if self.session.sources.workbook.sheet(target.sheet).is_none()
            || target.address.validate().is_err()
        {
            return Ok(CellValue::Error(crate::model::CellError::Ref));
        }
        let previous = self.session.state.runtime.stack.consumer.replace(target);
        self.session.state.runtime.budget.push_cell(target);
        if self.session.state.runtime.budget.profiling {
            self.session.state.runtime.run_usage.formula_executions = self
                .session
                .state
                .runtime
                .run_usage
                .formula_executions
                .saturating_add(1);
        }
        let result = (|| {
            self.session
                .state
                .runtime
                .budget
                .charge(crate::eval::limits::WorkKind::Expression, 1)?;
            self.session.state.runtime.budget.enter_depth()?;
            let result = adapter
                .call(
                    &mut CallContext {
                        session: &mut self.session,
                        target,
                    },
                    name,
                    parameters,
                )
                .map(|value| value.0);
            self.session.state.runtime.budget.leave_depth();
            result
        })()
        .and_then(|value| {
            ResultProjection::new(&mut self.session).finish_expression(
                target.sheet,
                target.address,
                value,
            )
        });
        self.session.state.runtime.budget.pop_cell();
        self.session.state.runtime.stack.consumer = previous;
        result
    }
}
