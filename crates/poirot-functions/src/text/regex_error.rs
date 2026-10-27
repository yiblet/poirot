/// Formula-domain failures remain distinct from backend and caller failures.
#[derive(Debug, thiserror::Error)]
pub enum RegexError<E> {
    /// Invalid PCRE2 pattern syntax.
    #[error("invalid pcre2 pattern syntax")]
    Pattern(#[source] pcre2::Error),
    /// No matching text, or no groups in capture-group mode.
    #[error("no matching text, or no groups in capture-group mode")]
    NoMatch,
    /// A result exceeds Excel's 32,767 UTF-16-unit text limit.
    #[error("a result exceeds excel's 32,767 utf-16-unit text limit")]
    TextLimit,
    /// A backend feature cannot preserve the model's text semantics.
    #[error("a backend feature cannot preserve the model's text semantics")]
    ByteEscape,
    /// Native match split a Unicode scalar.
    #[error("native match split a unicode scalar")]
    MatchBoundary,
    /// Native capture split a Unicode scalar.
    #[error("native capture split a unicode scalar")]
    CaptureBoundary,
    /// Native execution or compilation failed; this is never a valid no-match.
    #[error("native execution or compilation failed; this is never a valid no-match")]
    Native(#[source] pcre2::Error),
    /// A fallible allocation could not be admitted.
    #[error("a fallible allocation could not be admitted")]
    Resource,
    /// A checked byte or collection size cannot be represented.
    #[error("a checked byte or collection size cannot be represented")]
    SizeOverflow,
    /// Backend capture geometry did not satisfy the checked adapter state.
    #[error("backend capture geometry did not satisfy the checked adapter state")]
    InvalidState,
    /// The caller refused work or allocation.
    #[error("the caller refused work or allocation")]
    Host(#[source] E),
}
