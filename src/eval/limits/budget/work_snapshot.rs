use crate::eval::coordinates::EvaluationTarget;

/// Immutable consumed-work state used to decide whether a charge fits.
#[derive(Clone, Copy)]
pub(super) struct WorkSnapshot {
    /// Whether a cycle work quota is configured, including an explicit maximum integer.
    pub run_limited: bool,
    /// Whether a formula work quota is configured.
    pub cell_limited: bool,
    /// Work already consumed by the run.
    pub run_consumed: u64,
    /// Direct work consumed by the active formula, when present.
    pub cell_consumed: Option<u64>,
    /// Cell attribution for rejected work.
    pub target: Option<EvaluationTarget>,
}
