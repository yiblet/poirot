/// Classification of formula rejection for callers choosing a recovery policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParseErrorKind {
    /// Source cannot be represented by the supported grammar.
    Syntax,
    /// A structural or grid bound rejected source.
    ResourceLimit,
    /// Parser bounds are invalid.
    InvalidPolicy,
}
