//! Resolved builtin metadata carried into call admission.

use crate::eval::observe::ConsumedBuiltinInvocation;

/// Resolved invocation metadata and policy consumption needed before handler entry.
#[derive(Clone, Copy)]
pub(in crate::eval) struct ResolvedCall<'a> {
    /// Actual builtin invocation, absent for unresolved calls.
    pub invocation: Option<ConsumedBuiltinInvocation<'a>>,
    /// Whether name resolution depended on the selected builtin policy.
    pub read_builtin_policy: bool,
}
