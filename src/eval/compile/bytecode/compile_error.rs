/// Structural or resource failure while producing an executable program.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error,
)]
pub enum CompileError {
    /// Compilation exceeded a structural count or depth limit.
    #[error("bytecode structural resource limit exceeded")]
    ResourceLimit,
    /// An array was empty or had inconsistent row widths.
    #[error("array must be nonempty and rectangular")]
    InvalidArray,
    /// A reference exceeded workbook coordinate limits.
    #[error("reference exceeds worksheet bounds")]
    InvalidReference,
}
