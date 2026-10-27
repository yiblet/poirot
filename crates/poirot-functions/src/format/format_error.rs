//! Positioned and classified number-format failures.

use super::format_error_kind::FormatErrorKind;
use super::format_reason_error::FormatReasonError;

/// Number-format failure carrying its condition and original byte position.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("number format failed at byte {position}")]
pub struct FormatError {
    /// Programmatic invalid, unsupported, or resource classification.
    pub kind: FormatErrorKind,
    /// Original format byte offset when available.
    pub position: usize,
    /// Typed condition preventing the operation.
    #[source]
    pub reason: FormatReasonError,
}

/// Construct an invalid-format failure.
pub(super) fn error(reason: FormatReasonError) -> FormatError {
    diagnostic(FormatErrorKind::InvalidFormat, reason)
}

/// Construct an operational formatting resource-limit failure.
pub(super) fn resource_error(reason: FormatReasonError) -> FormatError {
    diagnostic(FormatErrorKind::ResourceLimit, reason)
}

/// Construct an unsupported UTF-16 fidelity failure.
pub(super) fn fidelity_error() -> FormatError {
    diagnostic(
        FormatErrorKind::UnsupportedFidelity,
        FormatReasonError::Utf8FormattingCannotRepresentUnpairedUtf16CodeUnits,
    )
}

fn diagnostic(kind: FormatErrorKind, reason: FormatReasonError) -> FormatError {
    FormatError {
        kind,
        position: 0,
        reason,
    }
}
