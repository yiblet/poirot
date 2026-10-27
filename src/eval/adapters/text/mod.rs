//! Text kernels keep algorithms independent from typed evaluator execution.
//!
//! Depends on: config (captured byte-text language), host (CallHost), session
//! (coercion and projection), vm (deferred arguments), functions (identities and
//! failures), limits (budgets) and values (value channels); sibling elementwise
//! (shared broadcast and collection kernels).

mod baht;

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
mod xml_kernel;
mod xml_selection_host;

pub(in crate::eval) use modern::execute as execute_modern;
pub(in crate::eval) use regex::execute as execute_regex;
pub(in crate::eval) use units::execute as execute_units;

pub(in crate::eval) use baht::execute as execute_baht;
pub(in crate::eval) use xml_kernel::execute as execute_xml;
