//! Failures while retaining external function names.

/// A stored external function name is unsupported, or its declared retention limit was exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalNamespaceError {
    /// Spelling requires an unsupported identifier grammar or case policy.
    UnsupportedName,
    /// Input entry count exceeded the declared retention limit.
    ResourceLimit,
}

impl std::fmt::Display for ExternalNamespaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "external function namespace: {self:?}")
    }
}

impl std::error::Error for ExternalNamespaceError {}
