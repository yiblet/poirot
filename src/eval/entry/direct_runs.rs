use super::recalculation::{formula_updates, recalculate};
use crate::eval::compile::bound;
use crate::eval::compile::compiler::{FormulaCompiler, PreparationPolicy};
use crate::eval::config::{EvaluationInputs, RunConfiguration};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{
    BudgetMeter, EvalResult, EvaluationBudgets, EvaluationOutcome, EvaluationUsage,
};
use crate::eval::run::EvaluationRun;
use crate::eval::session::{CellEvaluator, FormulaExecution, RunPolicies, Session};
use crate::eval::values::CellEvaluation;
use crate::model::{CellAddress, CellValue, Workbook};

/// Prepare a session over the workbook, run `operation` under validated budgets and finish the run.
pub(in crate::eval) fn execute_configuration<T>(
    workbook: &Workbook,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
    operation: impl FnOnce(&mut Session<'_>) -> EvalResult<T>,
) -> EvaluationOutcome<T> {
    // Validate budgets before preparation to preserve the compatibility error order.
    let meter = match BudgetMeter::new(budgets) {
        Ok(meter) => meter,
        Err(failure) => {
            return EvaluationOutcome::Aborted {
                failure,
                usage: EvaluationUsage::default(),
            };
        }
    };
    let plan = FormulaCompiler::new().compile_workbook(workbook, &PreparationPolicy::default());
    let mut session = Session::with_configuration(
        workbook,
        RunPolicies::from_run_configuration(configuration),
        meter,
    );
    session.sources.plan = Some(&plan);
    let mut run = EvaluationRun {
        session,
        failure: None,
    };
    let result = operation(&mut run.session);
    run.finish(result).outcome
}

/// Run one cell. Completed formula blanks publish as numeric zero; empty cells stay blank.
/// Semantic spreadsheet errors are successful `Complete` values.
pub fn run_cell(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_cell_with_configuration(workbook, sheet, address, &context.into(), budgets)
}

/// Evaluate one cell with explicit policies, preserving operational failures.
pub fn run_cell_with_configuration(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    execute_configuration(workbook, configuration, budgets, |session| {
        CellEvaluator::new(session).cell(sheet, address)
    })
}

/// Evaluate supplied expression text without publishing a worksheet formula result.
/// Blank expression results remain blank. Parsing/binding retain their own
/// preparation limits. Runtime work is metered separately.
pub fn run_formula(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_formula_with_configuration(workbook, sheet, address, formula, &context.into(), budgets)
}

/// Compile and execute supplied formula text with explicit policies and work limits.
pub fn run_formula_with_configuration(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    formula: &str,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    execute_configuration(workbook, configuration, budgets, |session| {
        let prepared =
            FormulaCompiler::new().compile_formula(formula, &PreparationPolicy::default());
        FormulaExecution::new(session).prepared_at(EvaluationTarget { sheet, address }, &prepared)
    })
}

/// Execute supplied bound syntax without reparsing; name bodies use prepared workbook plans.
pub fn run_plan(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    plan: &bound::FormulaPlan,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    run_plan_with_configuration(workbook, sheet, address, plan, &context.into(), budgets)
}

/// Execute supplied bound syntax with explicit policies and work limits.
pub fn run_plan_with_configuration(
    workbook: &Workbook,
    sheet: usize,
    address: CellAddress,
    plan: &bound::FormulaPlan,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<CellValue> {
    execute_configuration(workbook, configuration, budgets, |session| {
        FormulaExecution::new(session).bound_at(EvaluationTarget { sheet, address }, plan)
    })
}

/// Evaluate stored workbook formulas without applying their results to the model.
pub fn run_workbook(
    workbook: &Workbook,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<Vec<CellEvaluation>> {
    run_workbook_with_configuration(workbook, &context.into(), budgets)
}

/// Evaluate workbook formulas under explicit policies without publishing saved caches.
pub fn run_workbook_with_configuration(
    workbook: &Workbook,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<Vec<CellEvaluation>> {
    execute_configuration(workbook, configuration, budgets, |session| {
        formula_updates(workbook, session)
    })
}

/// Commit all completed caches under one exclusive borrow; an abort publishes none.
pub fn run_recalculation(
    workbook: &mut Workbook,
    context: EvaluationInputs,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<usize> {
    run_recalculation_with_configuration(workbook, &context.into(), budgets)
}

/// Evaluate and atomically publish formula caches under explicit policies.
pub fn run_recalculation_with_configuration(
    workbook: &mut Workbook,
    configuration: &RunConfiguration,
    budgets: EvaluationBudgets,
) -> EvaluationOutcome<usize> {
    match recalculate(workbook, configuration, budgets, false) {
        EvaluationOutcome::Complete {
            value: (count, _),
            usage,
        } => EvaluationOutcome::Complete {
            value: count,
            usage,
        },
        EvaluationOutcome::Aborted { failure, usage } => {
            EvaluationOutcome::Aborted { failure, usage }
        }
    }
}
