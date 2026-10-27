use super::shared_source_visitor::SharedSourceVisitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Formula text of a compiler-cache entry: borrowed from the compiler while capturing,
/// and decoded straight into the shared allocation the restored compiler keeps.
pub(super) enum CachedSource<'a> {
    /// Source key borrowed from the captured compiler.
    Borrowed(&'a str),
    /// Independently owned source shared by the pool key and its prepared formula.
    Shared(Arc<str>),
}

impl CachedSource<'_> {
    /// Borrow the exact formula text.
    pub(super) fn as_str(&self) -> &str {
        match self {
            Self::Borrowed(source) => source,
            Self::Shared(source) => source,
        }
    }

    /// Own the text, copying it only when it is still borrowed.
    pub(super) fn into_shared(self) -> Arc<str> {
        match self {
            Self::Borrowed(source) => Arc::from(source),
            Self::Shared(source) => source,
        }
    }
}

impl Serialize for CachedSource<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CachedSource<'_> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_str(SharedSourceVisitor)
            .map(Self::Shared)
    }
}
