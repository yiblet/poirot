//! Failure classifications used by formatting policy.

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
