/// Structural or resource failure while producing an executable program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompileError {
    /// Compilation exceeded a structural count or depth limit.
    ResourceLimit,
    /// An array was empty or had inconsistent row widths.
    InvalidArray,
    /// A reference exceeded workbook coordinate limits.
    InvalidReference,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bytecode compilation failed: {self:?}")
    }
}

impl std::error::Error for CompileError {}
