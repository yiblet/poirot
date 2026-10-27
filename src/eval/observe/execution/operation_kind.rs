use super::argument_origin::ArgumentOrigin;
use crate::eval::functions::registry::FunctionId;

/// An expression evaluation or one selected argument operation in the execution trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    /// Evaluation of an expression node.
    Expression {
        /// Builtin handler, when the expression is a resolved builtin call.
        handler: Option<FunctionId>,
    },
    /// Evaluation of one selected call argument.
    Argument {
        /// Zero-based syntactic argument position.
        ordinal: usize,
        /// Operand provenance before scalar projection.
        origin: ArgumentOrigin,
    },
}
