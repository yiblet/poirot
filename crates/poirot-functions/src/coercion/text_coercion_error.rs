use crate::coercion::CellError;

/// Formula text after host admission. Unicode projection failures remain distinct.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum TextCoercionError {
    /// Worksheet error propagated by the consumer.
    #[error("text conversion returned {0}")]
    Worksheet(CellError),
    /// Text cannot be projected without replacing isolated UTF16 surrogates.
    #[error("text conversion requires valid UTF-16")]
    UnsupportedText,
    /// An omitted argument needs a consumer-specific rule.
    #[error("text conversion requires an argument")]
    Missing,
}
