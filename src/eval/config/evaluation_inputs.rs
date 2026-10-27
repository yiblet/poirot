use super::context_fingerprint::ContextFingerprint;
use super::date_order::DateOrder;
use super::localization::Localization;
use std::time::SystemTime;

/// Clock, random seed, and locale settings for a calculation.
///
/// Workspace evaluation uses the complete [`EvaluationContext`](crate::eval::EvaluationContext) instead.
/// RAND and RANDBETWEEN use reproducible seed-and-cell-specific streams.
/// UTC is the default, rather than the host's local timezone.
#[derive(Clone, Copy, Debug)]
pub struct EvaluationInputs {
    /// Fixed instant used by date functions and extension contexts during this pass.
    /// Retained NOW/TODAY results reuse while this and their consumed conversion
    /// settings are unchanged; changing it invalidates actual clock consumers.
    pub as_of_time: SystemTime,
    /// Seed for reproducible cell-specific random streams. Retained draws reuse
    /// with unchanged source and seed; change the seed to request fresh draws.
    pub randomness_seed: u64,
    /// Date parsing and fixed-offset clock policy.
    pub localization: Localization,
}

impl EvaluationInputs {
    /// Capture a stable, bit-exact identity for these execution inputs.
    pub fn fingerprint(self) -> ContextFingerprint {
        ContextFingerprint {
            as_of_time: self.as_of_time,
            randomness_seed: self.randomness_seed,
            localization: self.localization,
        }
    }
}

impl Default for EvaluationInputs {
    fn default() -> Self {
        Self {
            as_of_time: SystemTime::now(),
            randomness_seed: 0,
            localization: Localization {
                utc_offset_seconds: 0,
                date_order: DateOrder::MonthDayYear,
            },
        }
    }
}
