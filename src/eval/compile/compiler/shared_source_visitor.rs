use serde::de::{Error, Visitor};
use std::sync::Arc;

/// Copies decoded formula text once, directly into a shared allocation.
pub(super) struct SharedSourceVisitor;

impl Visitor<'_> for SharedSourceVisitor {
    type Value = Arc<str>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("formula source text")
    }

    fn visit_str<E: Error>(self, source: &str) -> Result<Self::Value, E> {
        Ok(Arc::from(source))
    }
}
