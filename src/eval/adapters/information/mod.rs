//! Information family host integration.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! functions (identities and failures), limits (budgets) and values (value channels);
//! sibling elementwise (shared broadcast kernels).

/// Value type and spreadsheet error inspection handlers.
pub(in crate::eval) mod execution;
