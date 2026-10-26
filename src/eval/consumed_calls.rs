//! Caller-owned observation of actual builtin handler entry.
use crate::eval::{EvaluationFailure, EvaluationTarget, registry::FunctionId};

/// A builtin handler actually entered after expression and trace admission.
/// This is not a completed result: arguments or the handler can still abort.
/// Spellings come from the parsed AST and binding lookup, not raw lexeme spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsumedBuiltinInvocation<'call> {
    /// Formula cell entering the builtin handler.
    pub target: EvaluationTarget,
    /// Typed handler selected after binding and profile lookup.
    pub handler: FunctionId,
    /// AST function name; the parser may have normalized its case.
    pub parsed_spelling: &'call str,
    /// Exact spelling used by builtin lookup, including first stored external name.
    pub resolved_spelling: &'call str,
    /// Number of syntactic argument positions, including omissions.
    pub argument_count: usize,
}

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

#[cfg(test)]
use crate::eval::bound::{Arguments, Expression};

/// Extract an already bound builtin call without evaluating operands.
#[cfg(test)]
pub(in crate::eval) fn builtin_call(expression: &Expression) -> Option<(FunctionId, &Arguments)> {
    match expression {
        Expression::Date(call) => Some((FunctionId::Date(call.function), &call.arguments)),
        Expression::Logical(call) => Some((FunctionId::Logical(call.function), &call.arguments)),
        Expression::Statistics(call) => {
            Some((FunctionId::Statistics(call.function), &call.arguments))
        }
        Expression::Reference(call) => {
            Some((FunctionId::Reference(call.function), &call.arguments))
        }
        Expression::Search(call) => Some((FunctionId::Search(call.function), &call.arguments)),
        Expression::Information(call) => {
            Some((FunctionId::Information(call.function), &call.arguments))
        }
        Expression::Math(call) => Some((FunctionId::Math(call.function), &call.arguments)),
        Expression::Text(call) => Some((FunctionId::Text(call.function), &call.arguments)),
        Expression::Financial(call) => {
            Some((FunctionId::Financial(call.function), &call.arguments))
        }
        Expression::Engineering(call) => {
            Some((FunctionId::Engineering(call.function), &call.arguments))
        }
        Expression::Matrix(call) => Some((FunctionId::Matrix(call.function), &call.arguments)),
        Expression::Criteria(call) => Some((FunctionId::Criteria(call.function), &call.arguments)),
        Expression::Database(call) => Some((FunctionId::Database(call.function), &call.arguments)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "consumed_calls/tests.rs"]
mod tests;
