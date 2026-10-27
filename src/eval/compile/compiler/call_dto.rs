use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Call metadata using portable builtin spelling instead of internal slot numbers.
#[derive(Serialize, Deserialize)]
pub(super) struct CallDto<'a> {
    pub(super) source_offset: u32,
    pub(super) resolved_name: Option<Cow<'a, str>>,
    pub(super) builtin: Option<Cow<'a, str>>,
}
