//! Public direct entry points that evaluate cells, formulas and workbooks.
//!
//! Each `run_*` function prepares a session over a workbook, executes one request
//! under explicit budgets and returns an `EvaluationOutcome`. Bulk runs return
//! `CellEvaluation` outputs without changing the source cells; recalculation
//! publishes them to the workbook atomically.
//!
//! Depends on: session (sessions, cell evaluation and formula execution), compile
//! (formula compilation and bound plans), config (run configuration), functions
//! (extension registry and traces), limits (budgets and outcomes), coordinates
//! (targets), values (CellEvaluation outputs), run (evaluation runs) and
//! materialization (cache publication policy and receipts).

/// Public `run_*` entry points and the shared session runner.
mod direct_runs;
/// Calculation outcome with the registry identity and executed extension calls.
mod extension_evaluation;
/// Registry, captured context, and budgets for one extension-enabled evaluation.
mod extension_execution;
/// Extension-enabled `run_*_with_extensions` entry points.
mod extension_runs;
/// Whole-workbook recalculation with atomic cache publication.
mod recalculation;
#[cfg(test)]
mod tests;

pub use direct_runs::{
    run_cell, run_cell_with_configuration, run_formula, run_formula_with_configuration, run_plan,
    run_plan_with_configuration, run_recalculation, run_recalculation_with_configuration,
    run_workbook, run_workbook_with_configuration,
};
pub use extension_evaluation::ExtensionEvaluation;
pub use extension_execution::ExtensionExecution;
pub use extension_runs::{
    run_cell_with_extensions, run_formula_with_extensions, run_plan_with_extensions,
};
pub use recalculation::run_recalculation_with_receipts;
