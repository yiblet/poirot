/// Metadata describing a builtin's operand/arity evaluation protocol.
/// The family executor enforces the actual function-specific ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvaluationStrategy {
    /// Evaluate only operands selected by the handler.
    Lazy,
    /// Inspect eager operand errors before checking argument count.
    EagerErrorsBeforeArity,
    /// Let the family executor determine operand and error ordering.
    FamilyControlled,
}
