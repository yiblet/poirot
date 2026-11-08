/// Typed dependency failure from the selected regular-expression backend.
#[derive(Debug, thiserror::Error)]
pub enum RegexBackendError {
    /// PCRE2 compilation or execution failed.
    #[cfg(feature = "pcre2")]
    #[error("pcre2 backend failed")]
    Pcre2(#[from] pcre2::Error),
    /// Rust regex compilation failed.
    #[error("rust regex backend failed")]
    Rust(#[from] ::regex::Error),
}

impl RegexBackendError {
    /// Whether the backend explicitly reported an allocation failure.
    pub fn is_allocation_failure(&self) -> bool {
        match self {
            // PCRE2 public ABI: NOMEMORY=-48, HEAP_FAILED=121; source revision
            // and ABI provenance are recorded beside regex::compile_failure.
            #[cfg(feature = "pcre2")]
            Self::Pcre2(error) => matches!(error.code(), -48 | 121),
            Self::Rust(_) => false,
        }
    }
}
