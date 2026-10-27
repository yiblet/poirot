use super::preparation_policy::PreparationPolicy;
use super::prepared_formula::PreparedFormula;
use super::source_fingerprint::{SourceFingerprint, fingerprint};
use super::source_identity::SourceIdentity;
use crate::eval::coordinates::EvaluationTarget;
use crate::model::{CellAddress, Workbook};
use std::{collections::HashMap, sync::Arc};

/// Immutable compiled formula index and owned source topology; contains no cell results.
/// Exact equality, rather than a hash or generation,
/// protects reuse even when model fields are changed directly. Row visibility is
/// source metadata; ordinary values/styles are excluded.
#[derive(Clone, Debug)]
pub struct CompiledWorkbook {
    pub(super) identity: SourceIdentity,
    pub(super) fingerprint: SourceFingerprint,
    pub(super) policy: PreparationPolicy,
    pub(super) formulas: HashMap<(usize, CellAddress), Arc<PreparedFormula>>,
    pub(super) names: Vec<Arc<PreparedFormula>>,
}

impl CompiledWorkbook {
    /// Check exact source topology and preparation policy before reusing this plan.
    pub fn matches(&self, workbook: &Workbook, policy: &PreparationPolicy) -> bool {
        let identity = SourceIdentity::capture(workbook);
        &self.policy == policy
            && self.fingerprint == fingerprint(&identity)
            && self.identity == identity
    }

    /// Borrow the policy captured when this plan was compiled.
    pub fn policy(&self) -> &PreparationPolicy {
        &self.policy
    }

    /// Return the process-local source lookup hint.
    pub fn source_fingerprint(&self) -> SourceFingerprint {
        self.fingerprint
    }

    /// Find the prepared formula assigned to a stored formula cell.
    pub fn formula(&self, target: EvaluationTarget) -> Option<&Arc<PreparedFormula>> {
        self.formulas.get(&(target.sheet, target.address))
    }

    /// Find a prepared defined-name body by workbook name index.
    pub fn defined_name(&self, index: usize) -> Option<&Arc<PreparedFormula>> {
        self.names.get(index)
    }
}
