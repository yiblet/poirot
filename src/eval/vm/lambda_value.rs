use crate::eval::compile::bytecode::Compiled;
use crate::eval::values::{CallableBody, Value};

/// Immutable compiled lambda body and captured values retained for invocation.
pub(super) struct LambdaValue {
    /// Body's executable and concrete reference bindings.
    pub(super) program: Compiled,
    /// First instruction in the callable body.
    pub(super) entry: usize,
    /// Number of positional arguments.
    pub(super) arity: usize,
    /// Number of positional slots required before trailing parameters may be omitted.
    pub(super) minimum_arity: usize,
    /// Values copied from the defining lexical frame.
    pub(super) captures: Vec<Value>,
}

impl CallableBody for LambdaValue {}
