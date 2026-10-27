use crate::eval::reuse::{Dependencies, Key};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Serializable result paired with its consumed dependencies.
#[derive(Serialize, Deserialize)]
pub(super) struct CachedValue<'a> {
    pub(super) key: Key,
    pub(super) value: crate::model::snapshot::Value<'a>,
    pub(super) dependencies: Cow<'a, Dependencies>,
}
