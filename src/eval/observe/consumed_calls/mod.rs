//! Caller-owned observation of actual builtin handler entry.
//!
//! Depends on: functions (handler identities) and coordinates (targets) and limits
//! (failures).

/// Test-only extraction of already bound builtin calls.
#[cfg(test)]
mod builtin_call;
/// Caller-owned observer port for actual builtin handler entry.
mod builtin_invocation_observer;
/// A builtin handler actually entered after expression and trace admission.
mod consumed_builtin_invocation;
#[cfg(test)]
mod tests;

#[cfg(test)]
pub(in crate::eval) use builtin_call::builtin_call;
pub use builtin_invocation_observer::BuiltinInvocationObserver;
pub use consumed_builtin_invocation::ConsumedBuiltinInvocation;
