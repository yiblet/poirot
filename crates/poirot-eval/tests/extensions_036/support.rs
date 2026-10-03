use poirot_eval::{
    EvaluationBudgets, EvaluationInputs, EvaluationOutcome, extensions::*, registry::Arity,
};
use poirot_model::{CellAddress, CellValue, Workbook};

/// Build an eager deterministic test contract permitting all host capabilities.
pub(super) fn contract(arity: Arity) -> ExtensionContract {
    ExtensionContract {
        arity,
        evaluation: ExtensionEvaluationStrategy::Eager,
        capabilities: ExtensionCapabilities {
            references: true,
            arrays: true,
            context: true,
        },
        cacheability: ExtensionCacheability::Deterministic,
    }
}
/// Create the single-sheet baseline shared by extension integration tests.
pub(super) fn workbook() -> Workbook {
    let mut workbook = Workbook::new();
    workbook.add_sheet("Sheet1").unwrap();
    workbook
}
/// Execute formula text against the supplied test registry with default limits.
pub(super) fn run(
    workbook: &Workbook,
    registry: &ExtensionRegistry,
    formula: &str,
) -> ExtensionEvaluation<CellValue> {
    run_formula_with_extensions(
        workbook,
        0,
        CellAddress { row: 0, col: 0 },
        formula,
        ExtensionExecution {
            registry,
            context: EvaluationInputs::default(),
            budgets: EvaluationBudgets::default(),
        },
    )
}
/// Extract a completed extension test value, failing the test on operational abort.
pub(super) fn value(result: ExtensionEvaluation<CellValue>) -> CellValue {
    match result.outcome {
        EvaluationOutcome::Complete { value, .. } => value,
        outcome => panic!("{outcome:?}"),
    }
}
/// Test provider returning its first argument without changing its value channel.
pub(super) fn identity(host: &mut dyn ExtensionHost) -> ExtensionResult<ExtensionOutput> {
    host.charge_work(1)?;
    Ok(ExtensionOutput::Argument(0))
}
