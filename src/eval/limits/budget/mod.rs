//! Operational limits and accounting, separate from spreadsheet error values.
//!
//! ```
//! use poirot::eval::{EvaluationContext, EvaluationLimits};
//! use poirot::eval::overlay::Evaluation;
//! use poirot::model::Workbook;
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_formula("A1", "2*3")?;
//! let context = EvaluationContext::default().limits(EvaluationLimits::default().max_work(0));
//! let mut run = Evaluation::with_workbook(&book).build(context)?;
//! let output = run.target("Data!A1")?;
//! assert!(run.cell(output).is_err());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Depends on: coordinates (cell targets attributed to failures and charges).

/// The rejected charge, previously consumed amount, limit, and cell attribution.
mod budget_error;
/// The resource category that rejected an evaluation request.
mod budget_kind;
/// Mutable work accounting shared by nested evaluator operations.
mod budget_meter;
/// Stable identity for an explicitly unsupported execution capability.
mod capability_id;
/// Proposed work charge awaiting admission.
mod charge_request;
/// Internal result carrier keeping operational failures apart from cell errors.
mod eval_result;
/// Limits on calculation work and per-operation storage.
mod evaluation_budgets;
/// A failure that stops calculation rather than producing a spreadsheet error value.
mod evaluation_error;
/// Reusable, opt-in calculation quotas, independent of profiling and semantic context.
mod evaluation_limits;
/// Completion or operational failure of a calculation, with usage in either case.
mod evaluation_outcome;
mod evaluation_reason_error;
/// Work counters from a completed or failed calculation.
mod evaluation_usage;
/// Pure admission decisions for work charges, reservations and configured limits.
mod policy;
/// One cell's recorded random-stream consumption.
mod random_access;
/// Per-operation resource capacity checked separately from accumulated work.
mod reservation;
/// Operational cause distinct from a configured quota or unsupported spreadsheet feature.
mod resource_failure_kind;
#[cfg(test)]
mod tests;
/// Category of deterministic evaluator work, used for accounting rather than timing.
mod work_kind;
/// Consumed-work state used to decide whether a charge fits.
mod work_snapshot;

pub use budget_error::BudgetError;
pub use budget_kind::BudgetKind;
pub(in crate::eval) use budget_meter::BudgetMeter;
pub use capability_id::CapabilityId;
pub(in crate::eval) use eval_result::EvalResult;
pub use evaluation_budgets::EvaluationBudgets;
pub use evaluation_error::EvaluationError;
pub use evaluation_limits::EvaluationLimits;
pub use evaluation_outcome::EvaluationOutcome;
pub use evaluation_usage::EvaluationUsage;
pub use random_access::RandomAccess;
pub use resource_failure_kind::ResourceFailureKind;
pub use work_kind::WorkKind;

pub use evaluation_reason_error::EvaluationReasonError;
