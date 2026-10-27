use super::evaluation_limits::EvaluationLimits;

/// Limits on calculation work and per-operation storage.
///
/// Defaults are unlimited except for nesting depth, which is capped at 128.
/// `u64::MAX` and `usize::MAX` disable the corresponding limits; usage counters
/// saturate at those maxima. Work measures operations and visited data, not time.
/// Some kernels charge an estimated upper bound before running unmetered code.
///
/// `cell_work` counts work in one formula, excluding its dependency formulas.
/// Collection limits count elements in one collection, not total heap allocation.
/// Text limits count bytes in one variable buffer: UTF-8 bytes or two bytes per
/// UTF-16 unit. Fixed-size kernel scratch, including one run-local ordered
/// reduction state, is outside these limits.
///
/// Reused results still pass collection and text checks. Memo storage, tracing,
/// parsing, formatting, and file codecs have separate limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationBudgets {
    /// Maximum work charged across the whole evaluation.
    pub run_work: u64,
    /// Maximum direct work charged to one formula frame.
    pub cell_work: u64,
    /// Maximum nesting depth, subject to the engine's hard ceiling.
    pub depth: usize,
    /// Maximum logical elements admitted for any single collection.
    pub materialized_values: usize,
    /// Maximum states admitted for one wildcard match.
    pub wildcard_states: u64,
    /// Maximum byte storage admitted for one text buffer.
    pub text_bytes: usize,
}

impl Default for EvaluationBudgets {
    fn default() -> Self {
        Self {
            run_work: u64::MAX,
            cell_work: u64::MAX,
            depth: 128,
            materialized_values: usize::MAX,
            wildcard_states: u64::MAX,
            text_bytes: usize::MAX,
        }
    }
}

impl From<EvaluationLimits> for EvaluationBudgets {
    fn from(limits: EvaluationLimits) -> Self {
        Self {
            run_work: limits.work.unwrap_or(u64::MAX),
            cell_work: limits.formula_work.unwrap_or(u64::MAX),
            depth: limits.depth.unwrap_or(128),
            materialized_values: limits.collection_values.unwrap_or(usize::MAX),
            wildcard_states: limits.wildcard_states.unwrap_or(u64::MAX),
            text_bytes: limits.text_bytes.unwrap_or(usize::MAX),
        }
    }
}
