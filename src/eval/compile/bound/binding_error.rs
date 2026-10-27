//! Structural failure while admitting caller-provided expression syntax.

/// Structural failure while admitting caller-provided expression syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingError {
    /// Syntax exceeds supported node count or depth.
    ResourceLimit,
    /// Array syntax is empty or not rectangular.
    InvalidArray,
    /// Reference coordinates exceed worksheet bounds.
    InvalidReference,
}

impl std::fmt::Display for BindingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ResourceLimit => "formula binding resource limit exceeded",
            Self::InvalidArray => "array expressions must be nonempty and rectangular",
            Self::InvalidReference => "reference exceeds spreadsheet grid bounds",
        })
    }
}

impl std::error::Error for BindingError {}

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
