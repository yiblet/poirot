//! Unit tests for cached results, range indexes, recorded reads and lookup prefix snapshots.

/// Replacement with unchanged dependencies keeps indexes and updates versions.
mod cache_replacement;
/// Intrinsic cache size overflow under unlimited retention.
mod cache_retention_overflow;
/// Dirty storage accounting of duplicate and forced causes.
mod cache_validity_accounting;
/// Point reads suppressed by covering range memberships.
mod covered_point_reads;
/// Compressed read spans and value-ordered dirty causes.
mod dependency_value_order;
/// Decoding and validation of lookup prefix snapshots.
mod lookup_prefix_snapshots;
