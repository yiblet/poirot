/// Selection protocol whose operands remain conditional during execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConditionalArguments {
    /// Read the condition, then only branches selected by its scalar or array values.
    If,
    /// Read the value, then a fallback only for error elements.
    IfError,
    /// Read the value, then a fallback only for not-available elements.
    IfNa,
    /// Read indices, then only selected alternatives.
    Choose,
    /// Read conditions in order, selecting their corresponding results.
    Ifs,
    /// Read the selector and matching cases, then selected results or a default.
    Switch,
}
