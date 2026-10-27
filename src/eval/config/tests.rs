use super::clock::unix_seconds;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn fingerprints_retain_changes_smaller_than_floating_point_clock_resolution() {
    let first = crate::eval::config::evaluation_inputs::EvaluationInputs {
        as_of_time: UNIX_EPOCH + Duration::new(1_700_000_000, 1),
        ..Default::default()
    };
    let second = crate::eval::config::evaluation_inputs::EvaluationInputs {
        as_of_time: first.as_of_time + Duration::from_nanos(1),
        ..first
    };
    assert_eq!(
        unix_seconds(first.as_of_time),
        unix_seconds(second.as_of_time)
    );
    assert_ne!(first.fingerprint(), second.fingerprint());
    let mask = crate::eval::config::context_mask::ContextMask {
        as_of: true,
        ..Default::default()
    };
    assert_eq!(
        mask.changed_fields(first.fingerprint(), second.fingerprint(), false, false),
        vec![crate::eval::config::context_field::ContextField::AsOf]
    );
}

#[test]
fn numeric_conversion_handles_pre_epoch_fractional_seconds_at_the_leaf() {
    assert_eq!(unix_seconds(UNIX_EPOCH - Duration::from_millis(500)), -0.5);
    assert_eq!(unix_seconds(UNIX_EPOCH + Duration::from_millis(500)), 0.5);
}

/// Timestamp transport isolated from other fingerprint fields.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct ExactTime(#[serde(with = "crate::eval::config::timestamp")] SystemTime);

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
