use super::localization::Localization;
use std::time::SystemTime;

/// Complete context identity. Incremental evaluators should select the fields actually
/// used by each cell rather than invalidating deterministic cells on any change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ContextFingerprint {
    /// Exact captured instant, retaining nanoseconds and pre-epoch timestamps.
    #[serde(with = "crate::eval::config::timestamp")]
    pub as_of_time: SystemTime,
    /// Captured random seed.
    pub randomness_seed: u64,
    /// Captured localization policy.
    pub localization: Localization,
}
