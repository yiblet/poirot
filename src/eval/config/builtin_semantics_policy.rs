/// Builtin contracts selected at execution time. Compiled syntax and extension
/// bindings remain policy independent; the policy guards cached computation.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum BuiltinSemanticsPolicy {
    /// Preserve the existing registry and pinned source contracts.
    PinnedPoi,
    /// Documented Excel contracts. This is not an Excel observation/parity claim.
    #[default]
    ExcelDocumentation,
}
