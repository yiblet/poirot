//! Structural failure while admitting caller-provided expression syntax.

/// Structural failure while admitting caller-provided expression syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    /// Syntax exceeds supported node count or depth.
    #[error("binding structural resource limit exceeded")]
    ResourceLimit,
    /// Array syntax is empty or not rectangular.
    #[error("array must be nonempty and rectangular")]
    InvalidArray,
    /// Reference coordinates exceed worksheet bounds.
    #[error("reference exceeds worksheet bounds")]
    InvalidReference,
}

impl From<crate::eval::compile::bytecode::CompileError> for BindingError {
    fn from(error: crate::eval::compile::bytecode::CompileError) -> Self {
        match error {
            crate::eval::compile::bytecode::CompileError::ResourceLimit => Self::ResourceLimit,
            crate::eval::compile::bytecode::CompileError::InvalidArray => Self::InvalidArray,
            crate::eval::compile::bytecode::CompileError::InvalidReference => {
                Self::InvalidReference
            }
        }
    }
}
