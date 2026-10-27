//! Numeric conversion of captured evaluation instants for date arithmetic.

use std::time::{SystemTime, UNIX_EPOCH};

/// Convert a captured instant only when Excel's numeric date arithmetic requires it.
pub(in crate::eval) fn unix_seconds(time: SystemTime) -> f64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    }
}
