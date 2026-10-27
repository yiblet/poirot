//! Saved-result publication policy identity.

use super::cell_setter_source_binding::CellSetterSourceBinding;
use super::formula_cache_materialization_policy::FormulaCacheMaterializationPolicy;

/// Saved-result publication policy and its optional pinned source binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FormulaCacheMaterializationIdentity {
    /// Selected saved-result conversion policy.
    pub policy: FormulaCacheMaterializationPolicy,
    /// Pinned setter source identity, when selected.
    pub source: Option<CellSetterSourceBinding>,
}
