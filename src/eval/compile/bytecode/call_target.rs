/// Handler identity selected during compilation, without retaining a provider object.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CallTarget {
    /// Registered builtin handler.
    Builtin(crate::eval::functions::registry::FunctionId),
    /// Explicit extension registry binding.
    Extension(crate::eval::functions::extensions::ExtensionBinding),
    /// Unknown function, rejected only when execution reaches the call.
    Unresolved,
}
