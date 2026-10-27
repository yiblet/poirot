use crate::eval::limits::MemoRetentionLimits;
use crate::eval::session::expression_demand::ExpressionDemand;
use crate::eval::session::storage::Storage;

/// Mutable execution storage and the execution mode of the formula currently running.
pub(in crate::eval) struct RunState<'a> {
    /// Stacks, caches, budgets and effects owned by the run or a persistent workspace.
    pub(in crate::eval) runtime: Storage<'a>,
    /// Whether the active expression must produce a scalar or may keep an array.
    pub(in crate::eval) expression_demand: ExpressionDemand,
    /// Enabled by direct workspaces to publish only admissions from the current call.
    pub(in crate::eval) memo_limits: Option<MemoRetentionLimits>,
}

impl<'a> RunState<'a> {
    /// Start scalar execution over the given storage without memo retention limits.
    pub(in crate::eval) fn new(runtime: Storage<'a>) -> Self {
        Self {
            runtime,
            expression_demand: ExpressionDemand::default(),
            memo_limits: None,
        }
    }
}
