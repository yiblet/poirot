//! Dates family host integration.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments), config
//! (calendar and clock), functions (identities and failures), limits (budgets) and values
//! (value channels); sibling elementwise (shared broadcast kernels).

/// Date and time builtin execution with explicit context consumption.
pub(in crate::eval) mod execution;
