//! Outcome of looking up one external function spelling.

/// A preserved first spelling, definite absence, or an unsupported name comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalNameLookup<'a> {
    /// Exact first stored spelling selected by case-insensitive lookup.
    Stored(&'a str),
    /// Supported lookup found no stored spelling.
    Absent,
    /// No Java-compatible Unicode case policy is installed; do not guess a match.
    UnsupportedName,
}
