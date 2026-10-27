/// Registration namespace: a user-defined name or an explicitly registerable
/// Analysis ToolPak placeholder. Builtin implementations cannot be overridden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ExtensionNamespace {
    /// Caller-defined function outside the protected builtin namespace.
    UserDefined,
    /// Explicitly registerable Analysis ToolPak placeholder.
    AnalysisToolPak,
}
