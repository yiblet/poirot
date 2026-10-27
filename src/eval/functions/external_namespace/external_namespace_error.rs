//! Failures while retaining external function names.

/// A stored external function name is unsupported, or its declared retention limit was exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ExternalNamespaceError {
    /// Spelling requires an unsupported identifier grammar or case policy.
    #[error("unsupported external function name")]
    UnsupportedName,
    /// Input entry count exceeded the declared retention limit.
    #[error("external function retention limit exceeded")]
    ResourceLimit,
}
