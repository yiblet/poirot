//! Lossless Serde boundary for evaluation timestamps on both sides of the Unix epoch.

use serde::{Deserialize, Serialize, de::Error};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Exact signed duration relative to the epoch, independent of platform internals.
#[derive(Serialize, Deserialize)]
struct Timestamp {
    before_epoch: bool,
    seconds: u64,
    nanoseconds: u32,
}

/// Encode timestamp components without rounding through floating-point seconds.
pub(in crate::eval) fn serialize<S: serde::Serializer>(
    time: &SystemTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let (before_epoch, duration) = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => (false, duration),
        Err(error) => (true, error.duration()),
    };
    Timestamp {
        before_epoch,
        seconds: duration.as_secs(),
        nanoseconds: duration.subsec_nanos(),
    }
    .serialize(serializer)
}

/// Validate timestamp components and the target platform's representable range.
pub(in crate::eval) fn deserialize<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SystemTime, D::Error> {
    let timestamp = Timestamp::deserialize(deserializer)?;
    if timestamp.nanoseconds >= 1_000_000_000 {
        return Err(D::Error::custom("invalid timestamp nanoseconds"));
    }
    let duration = Duration::new(timestamp.seconds, timestamp.nanoseconds);
    let time = if timestamp.before_epoch {
        UNIX_EPOCH.checked_sub(duration)
    } else {
        UNIX_EPOCH.checked_add(duration)
    };
    time.ok_or_else(|| D::Error::custom("timestamp outside platform range"))
}
