//! Typed engineering execution through shared scalar and collection strategies.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments), functions
//! (identities and failures), limits (budgets) and values (value channels); sibling
//! elementwise (shared broadcast kernels).

/// Engineering family dispatch and its shared helpers.
mod dispatch;
/// Documented complex scalar execution and ordered collection reductions.
mod modern_complex;
/// Signed radix, bitwise and physical measurement execution.
mod modern_radix;
/// Documented bounded real engineering special functions.
mod modern_special;

pub(in crate::eval) use dispatch::execute_function;
