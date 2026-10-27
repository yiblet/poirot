/// When the engine resolves argument expressions before calling a provider.
/// Neither strategy automatically traverses all cells of a reference or array.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ExtensionEvaluationStrategy {
    /// Resolve argument expressions in order and check allowed kinds before the
    /// arity check and provider call. Reference contents remain lazy.
    Eager,
    /// Check arity first; resolve only arguments requested by the provider.
    /// Unrequested expressions, including errors or volatile calls, stay unevaluated.
    Deferred,
}
