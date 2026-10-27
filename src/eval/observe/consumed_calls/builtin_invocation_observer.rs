use super::consumed_builtin_invocation::ConsumedBuiltinInvocation;
use crate::eval::limits::EvaluationFailure;

/// Optional caller-owned run port. Capacity must be checked before copying an
/// event or its text. Borrowed delivery does not charge calculation allocation
/// or text budgets; the observer owns its retention admission.
/// Returning failure prevents handler effects and aborts the
/// run operationally. Earlier events remain useful diagnostic evidence on abort.
/// No event establishes parity without the retained complete backend run.
pub trait BuiltinInvocationObserver {
    /// Admit an actual handler entry before any handler effects occur.
    fn consumed(
        &mut self,
        invocation: ConsumedBuiltinInvocation<'_>,
    ) -> Result<(), EvaluationFailure>;
}
