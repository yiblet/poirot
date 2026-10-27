//! Reference family host integration.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments
//! and reference operators), compile (compiled argument views), functions (identities),
//! limits (budgets) and values (references); siblings elementwise (shared kernels) and
//! lookup (classic lookup execution).

/// Reference family dispatch and its shared helpers.
mod dispatch;
/// Reference-valued builtin execution and dynamic selection.
pub(in crate::eval) mod execution;
mod metadata;

pub(in crate::eval) use dispatch::execute;
