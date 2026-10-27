use super::expression::Expression;

/// Deferred operands of a bound function call with source and lookup spellings.
/// Binding selects a handler but does not evaluate these expressions.
#[derive(Clone, Debug, PartialEq)]
pub struct Arguments {
    /// Function spelling retained from parsed syntax.
    pub source_name: String,
    /// Exact lookup spelling when it differs from the parsed AST name.
    pub resolved_name: Option<String>,
    /// Deferred argument expressions in source order.
    pub expressions: Vec<Expression>,
}

impl Arguments {
    /// Construct manually bound arguments with the parsed spelling as lookup spelling.
    /// Manual inputs do not establish compiled-source evidence or backend parity.
    pub fn new(source_name: impl Into<String>, expressions: Vec<Expression>) -> Self {
        Self {
            source_name: source_name.into(),
            resolved_name: None,
            expressions,
        }
    }

    /// Retain exact binding lookup metadata; equal names need no second allocation.
    pub fn with_resolved_spelling(mut self, spelling: &str) -> Self {
        self.resolved_name = (spelling != self.source_name).then(|| spelling.to_owned());
        self
    }

    /// Return the exact spelling used for handler lookup.
    pub fn resolved_spelling(&self) -> &str {
        self.resolved_name.as_deref().unwrap_or(&self.source_name)
    }
}
