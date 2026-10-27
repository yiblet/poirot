//! Logical scalar kernels, reductions, and deferred branch selection.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments and branch
//! selection), functions (identities and failures), limits (budgets), values (value
//! channels) and observe (operation identities for error masking); sibling elementwise
//! (shared broadcast kernels).
mod aggregate;
/// Evaluator-owned logical builtin dispatch.
pub(in crate::eval) mod execution;
mod manual;
mod modern;
