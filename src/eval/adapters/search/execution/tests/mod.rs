//! Unit tests for session-backed lookup execution.

/// Classic MATCH and lookup selection.
mod classic_match;
/// Consumed sparse lookup absence in overlays.
mod incremental;
/// Prefix reuse across lookups.
mod prefix;
/// Session reads, limits and typed selection.
mod session_search;
/// Sparse lookup traversal.
mod sparse;
