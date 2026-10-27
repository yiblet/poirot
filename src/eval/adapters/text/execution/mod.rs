//! Family-owned evaluation; generic argument coercion remains in the execution core.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments), config (text
//! and formatting policies), functions (identities and failures), limits (budgets) and
//! values (value channels).

/// Text family dispatch.
mod dispatch;
/// TEXT formatting and render coercion.
mod formatting;
/// TEXTJOIN kernel that combines complete collections with repeating delimiters.
mod join_kernel;
/// TEXTJOIN output and the number of members joined so far.
mod joined;
/// NUMBERVALUE kernel that parses each broadcast text member using explicit or locale-default separators.
mod number_kernel;
mod operator_units;
/// Scalar text functions and the concatenation operator.
mod scalar;
/// Adapts scalar text functions to shared rectangular execution.
mod scalar_kernel;
/// Unit tests for text execution semantics.
#[cfg(test)]
mod tests;

pub(in crate::eval) use dispatch::execute_function;
pub(in crate::eval) use scalar::concat_operator;
