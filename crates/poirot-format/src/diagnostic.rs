//! Typed format failures preserve operational and text-fidelity boundaries.
use std::fmt;
/// Separates invalid format syntax from unsupported text fidelity and renderer resource failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatErrorKind {
    /// Malformed syntax or an invalid token/layout combination.
    InvalidFormat,
    /// Requested format behavior has no supported faithful implementation.
    UnsupportedFidelity,
    /// Compilation or rendering exceeds the explicit resource policy.
    ResourceLimit,
}
/// Number-format failure with a category callers can inspect through [`FormatError::kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError {
    /// Programmatic category distinguishing invalid, unsupported, and resource failures.
    pub kind: FormatErrorKind,
    /// Original format byte offset when available.
    pub position: usize,
    /// Explanation of the rejected format behavior.
    pub message: String,
}
impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.position)
    }
}
impl std::error::Error for FormatError {}
/// Construct an invalid-format diagnostic.
pub(super) fn error(message: &str) -> FormatError {
    diagnostic(FormatErrorKind::InvalidFormat, message)
}
/// Construct an operational formatting resource-limit diagnostic.
pub(super) fn resource_error(message: &str) -> FormatError {
    diagnostic(FormatErrorKind::ResourceLimit, message)
}
/// Construct an explicit unsupported-fidelity diagnostic.
pub(super) fn fidelity_error() -> FormatError {
    diagnostic(
        FormatErrorKind::UnsupportedFidelity,
        "UTF-8 formatting cannot represent unpaired UTF-16 code units",
    )
}
fn diagnostic(kind: FormatErrorKind, message: &str) -> FormatError {
    FormatError {
        kind,
        position: 0,
        message: message.into(),
    }
}
