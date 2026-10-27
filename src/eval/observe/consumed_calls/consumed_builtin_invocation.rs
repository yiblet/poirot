use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::FunctionId;

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
