/// Origin of a completed cell value within the current run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellProvenance {
    /// Computed from effective source content in this run.
    Evaluated,
    /// Reused from this run's memoized values.
    SessionMemo,
    /// Reused from a validated cross-run cache or caller-trusted source producer cache.
    ExternalCache,
}
