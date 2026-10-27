//! Tests for reads performed by the active cell.

/// Sheet qualifier and defined-name identifier comparison.
mod identifiers;
/// Borrowed source reads across saved ownership and sparse edits.
mod resolved_sources;
/// Sparse coordinate collection before referenced cell bodies.
mod sparse_collection;
/// Sparse member reads: retained sources, overrides, scan budgets and order.
mod sparse_members;
