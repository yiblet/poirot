//! Immutable shared source for independent runs with source-bound input batches.
use crate::eval::compile::compiler::CompiledWorkbook;
use crate::eval::config::RunConfiguration;
use crate::eval::inputs::{EffectiveWorkbook, RunInputs};
use crate::eval::limits::{EvaluationBudgets, EvaluationFailure};
use crate::eval::run::evaluation_run::EvaluationRun;
use crate::eval::run::retained_run::RetainedRun;
use crate::eval::run::shared_run_failure::SharedRunFailure;
use crate::eval::run::workbook_view::WorkbookView;
use crate::model::Workbook;
use std::sync::Arc;

/// Immutable workbook, checked compiled plan, and token binding its input batches.
struct SharedSource {
    workbook: Workbook,
    plan: Arc<CompiledWorkbook>,
    identity: Arc<()>,
}

/// Shares an immutable workbook and compiled plan across independent what-if runs.
/// Privately owns the source; callers only borrow its immutable model and plan.
/// Workbook values and compiled formulas are shared; input and memo state are per run.
/// Clone this handle for another caller; each prepared input batch remains bound to
/// the same source identity. The baseline workbook is never edited by a run.
///
/// ```
/// use poirot::eval::{EvaluationOutcome, EvaluationTarget, FormulaCompiler, PreparationPolicy, SharedWorkbook};
/// use poirot::model::{Workbook, CellValue, ModelError};
/// use std::sync::Arc;
/// let mut workbook = Workbook::new();
/// let sheet = workbook.add_sheet("Inputs")?;
/// sheet.set_value("A1".parse()?, 2.0)?;
/// sheet.set_formula("B1".parse()?, "A1*2")?;
/// let plan = FormulaCompiler::new().compile_workbook(&workbook, &PreparationPolicy::default());
/// let shared = SharedWorkbook::new(workbook, Arc::new(plan))?;
/// let (_, inputs) = shared.prepare_inputs(|_source| {
///     Ok::<_, ModelError>(((), vec![(0, "A1".parse()?, CellValue::Number(5.0))], 1))
/// }).expect("valid input batch");
/// let retained = shared.run_workbook(
///     &Default::default(), Default::default(), &inputs,
///     |run, _effective_source| run.evaluate_cell(EvaluationTarget {
///         sheet: 0, address: "B1".parse().unwrap(),
///     }),
/// ).expect("matching source and inputs");
/// assert!(matches!(retained.report.outcome, EvaluationOutcome::Complete {
///     value: CellValue::Number(10.0), ..
/// }));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct SharedWorkbook {
    source: Arc<SharedSource>,
}

impl SharedWorkbook {
    /// Validate and take ownership of an immutable workbook and matching plan.
    pub fn new(workbook: Workbook, plan: Arc<CompiledWorkbook>) -> Result<Self, EvaluationFailure> {
        WorkbookView::new(&workbook, &plan, plan.policy())?;
        Ok(Self {
            source: Arc::new(SharedSource {
                workbook,
                plan,
                identity: Arc::new(()),
            }),
        })
    }

    /// Type/admit against the immutable source before building its bound overlay.
    /// Caller ingress and model admission failures share the caller-selected error type.
    pub fn prepare_inputs<T, E: From<crate::model::ModelError>>(
        &self,
        prepare: impl FnOnce(
            &Workbook,
        ) -> Result<
            (
                T,
                Vec<(usize, crate::model::CellAddress, crate::model::CellValue)>,
                usize,
            ),
            E,
        >,
    ) -> Result<(T, RunInputs), E> {
        let source = &self.source;
        prepare(&source.workbook).and_then(|(value, assignments, maximum)| {
            RunInputs::new(
                &source.workbook,
                assignments,
                maximum,
                source.identity.clone(),
            )
            .map(|inputs| (value, inputs))
            .map_err(E::from)
        })
    }

    /// Evaluate against the immutable source with independent per-run state.
    /// The returned memo is moved out of Session.
    pub fn run_workbook<T>(
        &self,
        configuration: &RunConfiguration,
        budgets: EvaluationBudgets,
        inputs: &RunInputs,
        evaluate: impl FnOnce(
            &mut EvaluationRun<'_>,
            EffectiveWorkbook<'_>,
        ) -> Result<T, EvaluationFailure>,
    ) -> Result<RetainedRun<T>, SharedRunFailure> {
        self.run_workbook_with_optional_memo_limits(
            configuration,
            budgets,
            inputs,
            None,
            None,
            evaluate,
        )
    }

    /// Explicit retention admission; ordinary evaluation budgets remain independent.
    pub fn run_workbook_with_memo_limits<T>(
        &self,
        configuration: &RunConfiguration,
        budgets: EvaluationBudgets,
        inputs: &RunInputs,
        memo_limits: crate::eval::limits::MemoRetentionLimits,
        evaluate: impl FnOnce(
            &mut EvaluationRun<'_>,
            EffectiveWorkbook<'_>,
        ) -> Result<T, EvaluationFailure>,
    ) -> Result<RetainedRun<T>, SharedRunFailure> {
        self.run_workbook_with_optional_memo_limits(
            configuration,
            budgets,
            inputs,
            Some(memo_limits),
            None,
            evaluate,
        )
    }

    /// Run with borrowed observers against the immutable shared source.
    pub fn run_workbook_observed<T>(
        &self,
        configuration: &RunConfiguration,
        budgets: EvaluationBudgets,
        inputs: &RunInputs,
        memo_limits: crate::eval::limits::MemoRetentionLimits,
        observers: crate::eval::run::run_observers::RunObservers<'_>,
        evaluate: impl FnOnce(
            &mut EvaluationRun<'_>,
            EffectiveWorkbook<'_>,
        ) -> Result<T, EvaluationFailure>,
    ) -> Result<RetainedRun<T>, SharedRunFailure> {
        self.run_workbook_with_optional_memo_limits(
            configuration,
            budgets,
            inputs,
            Some(memo_limits),
            Some(observers),
            evaluate,
        )
    }

    fn run_workbook_with_optional_memo_limits<T>(
        &self,
        configuration: &RunConfiguration,
        budgets: EvaluationBudgets,
        inputs: &RunInputs,
        memo_limits: Option<crate::eval::limits::MemoRetentionLimits>,
        observers: Option<crate::eval::run::run_observers::RunObservers<'_>>,
        evaluate: impl FnOnce(
            &mut EvaluationRun<'_>,
            EffectiveWorkbook<'_>,
        ) -> Result<T, EvaluationFailure>,
    ) -> Result<RetainedRun<T>, SharedRunFailure> {
        let source = &self.source;
        if !Arc::ptr_eq(&source.identity, &inputs.source) {
            return Err(SharedRunFailure::ForeignInputs);
        }
        // Construction checked this exact pair once. Source fields are private,
        // all access is read-only, and neither handle can be replaced after that
        // check. As a child of run, this shell alone can build the trusted view.
        let view = WorkbookView {
            workbook: &source.workbook,
            plan: &source.plan,
        };
        let mut run = EvaluationRun::new_with_configuration(view, configuration, budgets)?
            .with_inputs(inputs);
        // This source is immutable for the whole session; prefixes never escape
        // this run, and observed executions keep streamed selection.
        run.session.state.runtime.lookup_prefixes.enable();
        if let Some(limits) = memo_limits {
            run = run.with_memo_limits(limits);
        }
        if let Some(observers) = observers {
            run = run
                .with_dependency_sink(observers.reads)
                .with_cell_observer(observers.cells)
                .with_builtin_invocation_observer(observers.builtins)
                .with_execution_observer(observers.operations);
        }
        let effective = EffectiveWorkbook::new(&source.workbook, Some(inputs));
        let result = evaluate(&mut run, effective);
        Ok(run.finish_retaining_cells(result))
    }

    /// Resolve canonical inputs under the same immutable baseline used by runs.
    /// Replay adapters retain responsibility for typed conversion and ingress limits.
    pub fn with_source<T>(&self, read: impl FnOnce(&Workbook) -> T) -> T {
        read(&self.source.workbook)
    }

    /// Read immutable source data and its already compiled plan.
    /// This does not materialize parsed trees or prepare any formulas.
    pub fn with_prepared<T>(&self, read: impl FnOnce(&Workbook, &CompiledWorkbook) -> T) -> T {
        read(&self.source.workbook, &self.source.plan)
    }
}
