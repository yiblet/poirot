use super::compiled_formula::CompiledFormula;
use super::scalar_locality::ScalarLocality;
use super::scalar_matcher::ScalarMatcher;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Reuse indexes for one compiler policy: proven non-spilling formula classes,
/// reference-rebinding candidates, and identical executable programs.
/// Candidate hashes narrow lookup; exact comparisons still authorize sharing.
#[derive(Default)]
// Hashes select candidate buckets only. Sharing still requires normalized-source
// equality for rebinding, or full executable equality for structural interning.
pub(super) struct TemplatePool {
    pub(super) scalar: HashSet<Vec<u8>>,
    pub(super) last_scalar: Option<ScalarMatcher>,
    pub(super) scalar_probation: Option<u64>,
    pub(super) classified: HashMap<u64, Vec<Arc<CompiledFormula>>>,
    pub(super) structural: HashMap<u64, Vec<Arc<crate::eval::compile::bytecode::Executable>>>,
}

impl TemplatePool {
    /// Require repeated literal-preserving locality before replacing the single matcher.
    pub(super) fn observe_scalar(
        &mut self,
        source: &str,
        locality: ScalarLocality,
        cells: &[(usize, usize)],
    ) {
        let ScalarLocality::Eligible(hint) = locality else {
            self.scalar_probation = None;
            return;
        };
        if self.scalar_probation == Some(hint) {
            if let Some(matcher) = &mut self.last_scalar {
                matcher.update(source, cells);
            } else {
                let mut matcher = ScalarMatcher::default();
                if matcher.update(source, cells) {
                    self.last_scalar = Some(matcher);
                }
            }
            self.scalar_probation = None;
        } else {
            self.scalar_probation = Some(hint);
        }
    }
}
