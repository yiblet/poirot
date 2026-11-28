use super::compiled_formula::CompiledFormula;
use std::sync::Arc;

/// A reusable executable with the stable hash used by the checkpoint wire format.
#[derive(Clone)]
pub(super) struct TemplateCandidate {
    pub(super) formula: Arc<CompiledFormula>,
    pub(super) snapshot_hash: u64,
}
