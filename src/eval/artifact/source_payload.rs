use crate::model::snapshot::Snapshot;
use serde::{Deserialize, Serialize};

/// Durable source payload independent of optional evaluator preparation.
// This source schema is a maintained file contract. Bump SOURCE_SCHEMA when changing it
// or the model snapshot wire layout; prepared engine changes need not invalidate source.
#[derive(Serialize, Deserialize)]
pub(super) struct SourcePayload<'a> {
    pub(super) model: Snapshot<'a>,
}
