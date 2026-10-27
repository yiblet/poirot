//! Search family host integration.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! spill (SpillDemand and SpillQuery for spilled lookup ranges), config (policies),
//! functions (identities), limits (budgets), values (value channels and references),
//! ports (dependency reads), inputs (effective workbook cells) and reuse (retained
//! lookup prefixes); sibling lookup (classic lookup execution).

/// Search family dispatch and its shared helpers.
mod dispatch;
/// Lookup admission, sparse candidate traversal, and result projection.
pub(in crate::eval) mod execution;
/// Source-ordered classic lookup handler.
mod handler;

pub(in crate::eval) use dispatch::execute;
