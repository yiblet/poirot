//! Exact evaluation timestamps; numeric conversion belongs to date/coercion leaves.
use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a captured instant only when Excel's numeric date arithmetic requires it.
pub(crate) fn unix_seconds(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    }
}

/// Lossless Serde boundary supporting timestamps on both sides of the Unix epoch.
pub(crate) mod timestamp {
    use super::*;
    use serde::{Deserialize, Serialize, de::Error};
    use std::time::Duration;

    /// Exact signed duration relative to the epoch, independent of platform internals.
    #[derive(Serialize, Deserialize)]
    struct Timestamp {
        before_epoch: bool,
        seconds: u64,
        nanoseconds: u32,
    }

    /// Encode timestamp components without rounding through floating-point seconds.
    pub(crate) fn serialize<S: serde::Serializer>(
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
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    /// Timestamp transport isolated from other fingerprint fields.
    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct ExactTime(#[serde(with = "timestamp")] SystemTime);

    #[test]
    fn serialization_preserves_epoch_direction_and_nanoseconds() {
        for time in [
            UNIX_EPOCH,
            UNIX_EPOCH - Duration::new(1, 123),
            UNIX_EPOCH + Duration::new(1_700_000_000, 987_654_321),
        ] {
            let bytes = postcard::to_allocvec(&ExactTime(time)).unwrap();
            let restored: ExactTime = postcard::from_bytes(&bytes).unwrap();
            assert_eq!(restored, ExactTime(time));
        }
    }

    #[test]
    fn serialization_rejects_invalid_nanoseconds_and_platform_overflow() {
        for wire in [(false, 0u64, 1_000_000_000u32), (false, u64::MAX, 0)] {
            let bytes = postcard::to_allocvec(&wire).unwrap();
            assert!(postcard::from_bytes::<ExactTime>(&bytes).is_err());
        }
    }

    #[test]
    fn fingerprints_retain_changes_smaller_than_floating_point_clock_resolution() {
        let first = crate::EvaluationInputs {
            as_of_time: UNIX_EPOCH + Duration::new(1_700_000_000, 1),
            ..Default::default()
        };
        let second = crate::EvaluationInputs {
            as_of_time: first.as_of_time + Duration::from_nanos(1),
            ..first
        };
        assert_eq!(
            unix_seconds(first.as_of_time),
            unix_seconds(second.as_of_time)
        );
        assert_ne!(first.fingerprint(), second.fingerprint());
        let mask = crate::context_dependencies::ContextMask {
            as_of: true,
            ..Default::default()
        };
        assert_eq!(
            mask.changed_fields(first.fingerprint(), second.fingerprint(), false, false),
            vec![crate::context_dependencies::ContextField::AsOf]
        );
    }

    #[test]
    fn numeric_conversion_handles_pre_epoch_fractional_seconds_at_the_leaf() {
        assert_eq!(unix_seconds(UNIX_EPOCH - Duration::from_millis(500)), -0.5);
        assert_eq!(unix_seconds(UNIX_EPOCH + Duration::from_millis(500)), 0.5);
    }
}
