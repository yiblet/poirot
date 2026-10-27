//! Evaluation runs: one execution lifetime over a prepared workbook view, with its
//! inputs, budgets, attached observers, memoized values and completion report.
//!
//! Use one [`EvaluationRun`] for several read-only targets so dependencies are
//! computed once in its memo. Compile again after formula/name/sheet topology changes;
//! a new run sees literal edits and starts with an empty memo. For reuse across
//! evaluations with edits, use the primary [`crate::eval::overlay::Overlay`] API. For parallel what-if requests sharing one
//! immutable source and plan, use [`SharedWorkbook`]; each run has
//! its own inputs, budgets, and memo.
//!
//! Repeated SUMIF and conformable SUMPRODUCT calls may share one bounded scalar
//! result within a run after each call prepares its own arguments. The first
//! repetition admits reuse; subsequent identical resolved calls skip the ordered
//! range scan. Edits and new evaluations discard this slot. External execution
//! observers retain ordinary reads. Work counters measure kernels actually entered.
//!
//! ```
//! use poirot::eval::{FormulaCompiler, PreparationPolicy, WorkbookView, EvaluationRun, EvaluationTarget};
//! use poirot::model::{Workbook, CellValue};
//! let mut workbook = Workbook::new();
//! let sheet = workbook.add_sheet("Sheet1")?;
//! sheet.set_value("A1".parse()?, 3.0)?;
//! sheet.set_formula("B1".parse()?, "A1*2")?;
//! sheet.set_formula("C1".parse()?, "B1+1")?;
//! sheet.set_formula("D1".parse()?, "B1+2")?;
//! let policy = PreparationPolicy::default();
//! let plan = FormulaCompiler::new().compile_workbook(&workbook, &policy);
//! let view = WorkbookView::new(&workbook, &plan, &policy)?;
//! let mut run = EvaluationRun::new(view, Default::default(), Default::default())?;
//! let c = run.evaluate_cell(EvaluationTarget { sheet: 0, address: "C1".parse()? })?;
//! let d = run.evaluate_cell(EvaluationTarget { sheet: 0, address: "D1".parse()? })?;
//! assert_eq!((c, d), (CellValue::Number(7.0), CellValue::Number(8.0)));
//! assert_eq!(run.usage().formula_executions, 3); // B1 is evaluated once.
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Depends on: session (sessions and cell evaluation), compile (compiled plans), config
//! (run configuration), functions (extensions), coordinates (targets and memo), limits
//! (budgets and failures), values (CellEvaluation outputs), adapters (family adapter
//! calls), ports (observer, read and cache ports), inputs (scenario inputs) and observe
//! (execution and builtin observers).

/// A requested cell, prepared formula, or bound expression evaluated at a target cell.
mod evaluation_probe;
/// Calculates several requested cells or formulas with shared budgets and memoized dependencies.
mod evaluation_run;
/// Owned results of one run.
mod retained_run;
/// Caller-supplied observers attached to the run's execution session.
mod run_observers;
/// A calculation outcome and its execution counters.
mod run_report;
/// Failure to start a shared-source run.
mod shared_run_failure;
/// Immutable workbook ownership for independent what-if runs.
mod shared_workbook;
/// Unit tests for run isolation, caching, observers and memo retention.
#[cfg(test)]
mod tests;
/// Pairs a workbook with a compiled plan validated against its formula topology and policy.
mod workbook_view;

pub use evaluation_probe::EvaluationProbe;
pub use evaluation_run::EvaluationRun;
pub use retained_run::RetainedRun;
pub use run_observers::RunObservers;
pub use run_report::RunReport;
pub use shared_run_failure::SharedRunFailure;
pub use shared_workbook::SharedWorkbook;
pub use workbook_view::WorkbookView;
