//! Operational limits: work budgets and usage records, run counters, memo retention
//! limits, operational failures and outcomes, and fallible allocation.
//!
//! Depends on: coordinates (cell targets attributed to failures and charges).

/// Work limits, usage counters, and operational evaluation failures.
mod budget;
/// Logical bounds on owned Session memo retention, independent of calculation budgets.
mod memo_retention_limits;
/// Logical memo entries and retained bytes, using the same units as [`MemoRetentionLimits`].
mod memo_retention_usage;
/// Fallible storage admission.
mod resource;
/// Execution counters for one run.
mod run_usage;

pub use budget::{
    BudgetError, BudgetKind, CapabilityId, EvaluationBudgets, EvaluationError, EvaluationLimits,
    EvaluationOutcome, EvaluationReasonError, EvaluationUsage, RandomAccess, ResourceFailureKind,
    WorkKind,
};
pub(in crate::eval) use budget::{BudgetMeter, EvalResult};
pub use memo_retention_limits::MemoRetentionLimits;
pub use memo_retention_usage::MemoRetentionUsage;
pub(in crate::eval) use resource::reserve_exact;
pub use run_usage::RunUsage;
