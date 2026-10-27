//! Versioned calculation-owned streams; no platform hashes or ambient RNG.
//!
//! Depends on: no other eval folder.

/// Inclusive integer bounds admitted for unbiased integer draws.
mod interval;
/// Reproducible per-cell generator and its versioned work constants.
mod stream;

pub(in crate::eval) use interval::Interval;
pub(in crate::eval) use stream::{INTEGER_WORK, STREAM_VERSION, Stream};
