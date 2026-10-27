//! Owned or borrowed access to persistent execution storage.

use super::runtime::Runtime;

/// Execution storage owned by a run or borrowed from a persistent workspace; both
/// dereference to the same `Runtime`, so callers use one field path either way.
pub(in crate::eval) enum Storage<'a> {
    Owned(Box<Runtime>),
    Borrowed(&'a mut Runtime),
}

impl std::ops::Deref for Storage<'_> {
    /// Persistent execution storage reached through this borrowed or owned port.
    type Target = Runtime;

    fn deref(&self) -> &Runtime {
        match self {
            Self::Owned(state) => state,
            Self::Borrowed(state) => state,
        }
    }
}

impl std::ops::DerefMut for Storage<'_> {
    fn deref_mut(&mut self) -> &mut Runtime {
        match self {
            Self::Owned(state) => state,
            Self::Borrowed(state) => state,
        }
    }
}
