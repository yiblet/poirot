/// The resource category that rejected an evaluation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetKind {
    /// Invalid or unsupported budget configuration.
    Configuration,
    /// Total run work limit.
    RunWork,
    /// Per-formula direct work limit.
    CellWork,
    /// Nesting depth limit.
    Depth,
    /// Per-collection element limit.
    Materialization,
    /// Per-match wildcard state limit.
    WildcardStates,
    /// Per-buffer text storage limit.
    TextBytes,
}
