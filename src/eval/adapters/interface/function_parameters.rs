use crate::eval::compile::bytecode::Compiled;
use crate::eval::vm::ArgumentsRef;

/// Borrowed lazy argument blocks from compiled bytecode.
/// This view never materializes a bytecode tree just to call a function family.
#[derive(Clone, Copy)]
pub struct FunctionParameters<'a>(
    /// Deferred bytecode argument handles.
    pub(in crate::eval) ArgumentsRef<'a>,
);

impl<'a> From<&'a [Compiled]> for FunctionParameters<'a> {
    fn from(expressions: &'a [Compiled]) -> Self {
        Self(expressions.into())
    }
}

impl FunctionParameters<'_> {
    /// Count syntactic argument positions, including omissions.
    pub fn len(self) -> usize {
        self.0.len()
    }

    /// Whether the call has no argument positions.
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}
