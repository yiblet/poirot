//! Extension execution entry points and compiled-plan lifetimes.
use super::extension_evaluation::ExtensionEvaluation;
use super::extension_execution::ExtensionExecution;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::extensions::ExtensionTrace;
use crate::eval::limits::EvaluationFailure;
use crate::eval::session::{CellEvaluator, FormulaExecution};
use crate::model::{CellAddress, CellValue};

/// Prepare workbook sources and execute one bounded operation with extension tracing.
fn run<T>(
    workbook: &crate::model::Workbook,
    execution: ExtensionExecution<'_>,
    operation: impl FnOnce(&mut crate::eval::session::Session<'_>) -> crate::eval::limits::EvalResult<T>,
) -> ExtensionEvaluation<T> {
    let identity = execution.registry.identity();
    let meter = match crate::eval::limits::BudgetMeter::new(execution.budgets) {
        Ok(meter) => meter,
        Err(failure) => {
            return ExtensionEvaluation {
                outcome: crate::eval::limits::EvaluationOutcome::Aborted {
                    failure,
                    usage: crate::eval::limits::EvaluationUsage::default(),
                },
                registry_identity: identity,
                trace: ExtensionTrace::default(),
            };
        }
    };
    let policy = crate::eval::compile::compiler::PreparationPolicy::with_extensions(
        poirot_parsing::formula::FormulaParsePolicy::default(),
        execution.registry,
    );
    let plan = crate::eval::compile::compiler::FormulaCompiler::new()
        .compile_workbook_with_extensions(workbook, &policy, execution.registry);
    let mut session = crate::eval::session::Session::new(workbook, execution.context, meter);
    session.sources.extensions = Some(execution.registry);
    session.sources.plan = Some(&plan);
    let result = operation(&mut session);
    let trace = std::mem::take(&mut session.state.runtime.effects.extension_trace);
    ExtensionEvaluation {
        outcome: session.state.runtime.budget.take().outcome(result),
        registry_identity: identity,
        trace,
    }
}

/// Registered UDFs are available in dependency formulas and defined names too.
pub fn run_cell_with_extensions(
    workbook: &crate::model::Workbook,
    sheet: usize,
    address: CellAddress,
    execution: ExtensionExecution<'_>,
) -> ExtensionEvaluation<CellValue> {
    run(workbook, execution, |session| {
        CellEvaluator::new(session).cell(sheet, address)
    })
}

/// Evaluate supplied formula text at a caller cell using this registry.
/// The workbook is borrowed read-only; no saved results are published. Spreadsheet
/// errors are completed cell values, while host/capability failures abort the outcome.
pub fn run_formula_with_extensions(
    workbook: &crate::model::Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    execution: ExtensionExecution<'_>,
) -> ExtensionEvaluation<CellValue> {
    run(workbook, execution, |session| {
        let policy = crate::eval::compile::compiler::PreparationPolicy::with_extensions(
            poirot_parsing::formula::FormulaParsePolicy::default(),
            session
                .sources
                .extensions
                .ok_or(EvaluationFailure::StalePlan)?,
        );
        let prepared = crate::eval::compile::compiler::FormulaCompiler::new()
            .compile_formula_with_extensions(
                formula,
                &policy,
                session
                    .sources
                    .extensions
                    .ok_or(EvaluationFailure::StalePlan)?,
            );
        FormulaExecution::new(session).prepared_at(EvaluationTarget { sheet, address }, &prepared)
    })
}

/// Execute an extension-bound syntax plan without reparsing it.
/// A registry identity mismatch aborts; rebind the plan after registration changes.
pub fn run_plan_with_extensions(
    workbook: &crate::model::Workbook,
    sheet: usize,
    address: CellAddress,
    plan: &crate::eval::compile::bound::FormulaPlan,
    execution: ExtensionExecution<'_>,
) -> ExtensionEvaluation<CellValue> {
    let identity_matches = plan
        .extension_identity()
        .is_none_or(|identity| identity == &execution.registry.identity());
    run(workbook, execution, |session| {
        if !identity_matches {
            return Err(EvaluationFailure::UnsupportedCapability {
                capability: crate::eval::limits::CapabilityId::UserDefinedFunction,
                source: Some(EvaluationTarget { sheet, address }),
                detail: "extension registry identity changed; rebind the formula plan".into(),
            });
        }
        FormulaExecution::new(session).bound_at(EvaluationTarget { sheet, address }, plan)
    })
}
