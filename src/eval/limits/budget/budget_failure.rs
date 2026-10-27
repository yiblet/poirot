use super::budget_kind::BudgetKind;
use crate::eval::coordinates::EvaluationTarget;

/// The rejected charge, previously consumed amount, limit, and cell attribution.
/// `requested` is the additional charge that would exceed the limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetFailure {
    /// Resource category that rejected the charge.
    pub kind: BudgetKind,
    /// Configured maximum for that category.
    pub limit: u64,
    /// Amount already consumed before the rejected charge.
    pub consumed: u64,
    /// Additional amount requested by the rejected operation.
    pub requested: u64,
    /// Formula cell executing when the charge failed, when available.
    pub target: Option<EvaluationTarget>,
}
