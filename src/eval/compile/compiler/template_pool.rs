use super::compiled_formula::CompiledFormula;
use super::scalar_locality::ScalarLocality;
use super::scalar_matcher::ScalarMatcher;
use super::template_candidate::TemplateCandidate;
use super::template_lexemes::TemplateLexemes;
use super::templates::{rebound, reusable_template};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Reuse indexes for one compiler policy: proven non-spilling formula classes,
/// reference-rebinding candidates, and identical executable programs.
/// Exact token equality authorizes rebinding. Structural interning checks executable equality.
#[derive(Clone, Default)]
pub(super) struct TemplatePool {
    pub(super) scalar: HashSet<Vec<u8>>,
    pub(super) last_scalar: Option<ScalarMatcher>,
    pub(super) scalar_probation: Option<u64>,
    pub(super) classified: HashMap<Vec<u8>, TemplateCandidate>,
    /// Restored wire hints awaiting their first use by a distinct source.
    pub(super) restored: HashMap<u64, Vec<Arc<CompiledFormula>>>,
    pub(super) structural: HashMap<u64, Vec<Arc<crate::eval::compile::bytecode::Executable>>>,
}

impl TemplatePool {
    /// Rebind a complete source signature, loading a matching restored bucket only on a miss.
    pub(super) fn rebind(
        &mut self,
        source: &str,
        lexemes: &TemplateLexemes,
    ) -> Option<CompiledFormula> {
        if let Some(candidate) = self.classified.get(lexemes.key.as_slice()) {
            return rebound(&candidate.formula, source, &lexemes.cells);
        }
        if self.restored.is_empty() {
            return None;
        }
        let hash = poirot_parsing::formula::classified::normalized_hash(source).ok()?;
        let bucket = self.restored.remove(&hash)?;
        let mut restored_lexemes = TemplateLexemes::default();
        for formula in bucket {
            if restored_lexemes
                .scan(&formula.source, &formula.policy)
                .is_some()
            {
                self.admit_template(&restored_lexemes, &formula, Some(hash));
            }
        }
        let candidate = self.classified.get(lexemes.key.as_slice())?;
        rebound(&candidate.formula, source, &lexemes.cells)
    }

    /// Retain one complete exact signature for an eligible fresh or restored exemplar.
    pub(super) fn admit_template(
        &mut self,
        lexemes: &TemplateLexemes,
        formula: &Arc<CompiledFormula>,
        stored_hint: Option<u64>,
    ) {
        if !reusable_template(formula, lexemes) {
            return;
        }
        let Ok(snapshot_hash) =
            poirot_parsing::formula::classified::normalized_hash(&formula.source)
        else {
            return;
        };
        if stored_hint.is_some_and(|hint| hint != snapshot_hash) {
            return;
        }
        let mut key = Vec::new();
        if key.try_reserve_exact(lexemes.key.len()).is_err()
            || self.classified.try_reserve(1).is_err()
        {
            return;
        }
        // SAFETY: Each reservation admitted the key bytes or the new index entry.
        key.extend_from_slice(&lexemes.key);
        self.classified
            .entry(key)
            .or_insert_with(|| TemplateCandidate {
                formula: Arc::clone(formula),
                snapshot_hash,
            });
    }

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
