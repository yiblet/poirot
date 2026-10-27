//! Typed financial kernels over deferred scalar and whole-collection roles.
//!
//! Depends on: host (CallHost), session (coercion), vm (deferred arguments), functions
//! (identities), limits (budgets), values (value channels and failures) and ports
//! (dependency reads).

mod cashflow;
/// Financial family dispatch and its shared helpers.
mod dispatch;
mod euro;
mod modern;
mod odd;
mod scalar;
mod securities;

pub(in crate::eval) use dispatch::execute_function;
