use super::conditional_arguments::ConditionalArguments;

/// Operand-demand contract; it never grants permission to reorder errors or effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ArgumentEvaluation {
    /// A builtin selects operands according to its declared conditional protocol.
    Conditional(ConditionalArguments),
    /// The builtin inspects eager operand errors before its arity check.
    EagerErrorsBeforeArity,
    /// A family executor or external provider controls demand and error ordering.
    HandlerControlled,
}
