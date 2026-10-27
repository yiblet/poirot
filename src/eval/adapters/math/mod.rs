//! Mathematical kernels using shared typed scalar and collection execution.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments), config
//! (random streams and policies), functions (identities and failures), limits (budgets)
//! and values (value channels); sibling elementwise (shared broadcast kernels).
/// Evaluator-owned mathematical builtin dispatch.
pub(in crate::eval) mod execution;
mod modern;
mod radix;
mod series;
