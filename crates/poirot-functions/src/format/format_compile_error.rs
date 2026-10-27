//! Number-format compilation failures with token context.

use super::format_error::FormatError;

/// An error compiling a number format, with its section and token when known.
///
/// Errors from parsing format text also retain the original UTF-8 byte offset.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("could not compile number format at section {section:?}, token {token:?}")]
pub struct FormatCompileError {
    /// Zero-based failing section index, if known.
    pub section: Option<usize>,
    /// Zero-based failing token index within the section, if known.
    pub token: Option<usize>,
    /// Underlying syntax, fidelity, or resource diagnostic.
    pub source: FormatError,
}
impl From<FormatError> for FormatCompileError {
    fn from(source: FormatError) -> Self {
        Self {
            section: None,
            token: None,
            source,
        }
    }
}
