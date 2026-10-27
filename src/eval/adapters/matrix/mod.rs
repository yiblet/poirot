//! Matrix adapters use shared typed rectangle services and independent numeric algorithms.
//!
//! Depends on: host (CallHost), session (reads and coercion), vm (deferred arguments),
//! config (policies), functions (identities and failures), limits (budgets and copying)
//! and values (value channels); sibling elementwise (shared broadcast kernels).

/// Matrix family dispatch and its shared helpers.
mod dispatch;
mod grouping;
mod modern;
mod pivot_axis;
mod pivot_executor;
mod random_trim;
mod regression;
mod selection;
/// Unit tests for lossless matrix adapter ports.
#[cfg(test)]
mod tests;
mod unit;

pub(in crate::eval) use dispatch::execute;
