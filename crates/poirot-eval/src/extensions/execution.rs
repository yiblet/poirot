//! Extension execution entry points and compiled-plan lifetimes.
use super::{ExtensionBinding, ExtensionIdentity, ExtensionRegistry};
use crate::{
    CellAddress, CellValue, EvaluationInputs, EvaluationTarget, budget::EvaluationFailure,
};

/// Consumed function contracts, for invalidation without spelling heuristics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionInvocation {
    /// Formula cell that entered the provider.
    pub target: EvaluationTarget,
    /// Exact executed registration and declared contract.
    pub binding: ExtensionBinding,
}
/// Executed extension calls in invocation order. Entries show entry, not successful completion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtensionTrace {
    /// Actual provider entries in execution order.
    pub invocations: Vec<ExtensionInvocation>,
}
/// Calculation outcome together with the exact registry identity and executed extension calls.
/// An aborted outcome can retain a partial invocation trace.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtensionEvaluation<T> {
    /// Completed result or operational abort with usage.
    pub outcome: crate::EvaluationOutcome<T>,
    /// Registry identity used to bind and execute this run.
    pub registry_identity: ExtensionIdentity,
    /// Actual provider invocations, possibly partial after an abort.
    pub trace: ExtensionTrace,
}

/// Registry, captured context, and budgets for one extension-enabled evaluation.
/// Use with [`run_formula_with_extensions`] or [`run_cell_with_extensions`]; the
/// overlay builder accepts the runtime extension registry.
pub struct ExtensionExecution<'a> {
    /// Installed providers and exact binding identities.
    pub registry: &'a ExtensionRegistry,
    /// Captured clock, seed, and localization inputs.
    pub context: EvaluationInputs,
    /// Execution work and allocation limits.
    pub budgets: crate::EvaluationBudgets,
}
/// Prepare workbook sources and execute one bounded operation with extension tracing.
pub(crate) fn run<T>(
    workbook: &crate::Workbook,
    execution: ExtensionExecution<'_>,
    operation: impl FnOnce(&mut crate::Session<'_>) -> crate::budget::EvalResult<T>,
) -> ExtensionEvaluation<T> {
    let identity = execution.registry.identity();
    let meter = match crate::budget::BudgetMeter::new(execution.budgets) {
        Ok(meter) => meter,
        Err(failure) => {
            return ExtensionEvaluation {
                outcome: crate::EvaluationOutcome::Aborted {
                    failure,
                    usage: crate::EvaluationUsage::default(),
                },
                registry_identity: identity,
                trace: ExtensionTrace::default(),
            };
        }
    };
    let policy = crate::EvaluationPolicy::with_extensions(
        poirot_parsing::formula::FormulaParsePolicy::default(),
        execution.registry,
    );
    let plan = crate::FormulaCompiler::new().compile_workbook_with_extensions(
        workbook,
        &policy,
        execution.registry,
    );
    let mut session = crate::Session::new(workbook, execution.context, meter);
    session.extensions = Some(execution.registry);
    session.plan = Some(&plan);
    let result = operation(&mut session);
    let trace = std::mem::take(&mut session.runtime.extension_trace);
    ExtensionEvaluation {
        outcome: session.runtime.budget.take().outcome(result),
        registry_identity: identity,
        trace,
    }
}
/// Registered UDFs are available in dependency formulas and defined names too.
pub fn run_cell_with_extensions(
    workbook: &crate::Workbook,
    sheet: usize,
    address: CellAddress,
    execution: ExtensionExecution<'_>,
) -> ExtensionEvaluation<CellValue> {
    run(workbook, execution, |session| session.cell(sheet, address))
}
/// Evaluate supplied formula text at a caller cell using this registry.
/// The workbook is borrowed read-only; no saved results are published. Spreadsheet
/// errors are completed cell values, while host/capability failures abort the outcome.
pub fn run_formula_with_extensions(
    workbook: &crate::Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    execution: ExtensionExecution<'_>,
) -> ExtensionEvaluation<CellValue> {
    run(workbook, execution, |session| {
        let policy = crate::EvaluationPolicy::with_extensions(
            poirot_parsing::formula::FormulaParsePolicy::default(),
            session.extensions.ok_or(EvaluationFailure::StalePlan)?,
        );
        let prepared = crate::FormulaCompiler::new().compile_formula_with_extensions(
            formula,
            &policy,
            session.extensions.ok_or(EvaluationFailure::StalePlan)?,
        );
        session.prepared_at(EvaluationTarget { sheet, address }, &prepared)
    })
}
/// Execute an extension-bound syntax plan without reparsing it.
/// A registry identity mismatch aborts; rebind the plan after registration changes.
pub fn run_plan_with_extensions(
    workbook: &crate::Workbook,
    sheet: usize,
    address: CellAddress,
    plan: &crate::bound::FormulaPlan,
    execution: ExtensionExecution<'_>,
) -> ExtensionEvaluation<CellValue> {
    let identity_matches = plan
        .extension_identity()
        .is_none_or(|identity| identity == &execution.registry.identity());
    run(workbook, execution, |session| {
        if !identity_matches {
            return Err(EvaluationFailure::UnsupportedCapability {
                capability: crate::CapabilityId::UserDefinedFunction,
                source: Some(EvaluationTarget { sheet, address }),
                detail: "extension registry identity changed; rebind the formula plan".into(),
            });
        }
        session.bound_at(EvaluationTarget { sheet, address }, plan)
    })
}
