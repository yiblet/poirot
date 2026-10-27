//! Contextual aggregation syntax, distinct from ordinary worksheet values: the
//! identifiers a grouping call's aggregation argument names, lowered without
//! treating them as worksheet values.
//!
//! Depends on: functions (extension constructor precedence); parent bytecode
//! (compile errors and nesting depth).

/// Scoped identifier awaiting its aggregation role.
mod candidate;
/// Aggregation-role projection attached to a bytecode entry.
mod entry;
/// Lowering of aggregation-role syntax into plans.
mod lowering;
/// Identifiers lowered from one aggregation argument.
mod plan;
/// Unit tests for aggregation-role lowering.
#[cfg(test)]
mod tests;

pub use candidate::Candidate;
pub use entry::Entry;
pub use lowering::lower;
#[cfg(test)]
pub(in crate::eval) use lowering::lower_tree;
pub(in crate::eval) use lowering::lower_with_registry;
pub use plan::Plan;
