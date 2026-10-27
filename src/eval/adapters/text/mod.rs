//! Text kernels keep algorithms independent from typed evaluator execution.
//!
//! Depends on: host (CallHost), session (coercion and projection), vm (deferred
//! arguments), config (text and formatting policies), functions (identities and
//! failures), limits (budgets) and values (value channels); sibling elementwise (shared
//! broadcast and collection kernels).

/// Evaluator-owned text builtin dispatch.
pub(in crate::eval) mod execution;
mod modern;
/// Conversion of text family results into evaluator results.
mod outcome;
mod regex;
/// Text family tests.
#[cfg(test)]
mod tests;
mod units;

pub(in crate::eval) use modern::execute as execute_modern;
pub(in crate::eval) use regex::execute as execute_regex;
pub(in crate::eval) use units::execute as execute_units;
