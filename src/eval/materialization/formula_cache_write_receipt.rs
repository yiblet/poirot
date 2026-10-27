//! Evidence of one prepared formula-cache write.

use super::formula_cache_materialization_identity::FormulaCacheMaterializationIdentity;
use super::materialized_scalar_summary::MaterializedScalarSummary;
use crate::eval::coordinates::EvaluationTarget;

/// Evidence of one prepared formula-cache write: target, policy, computed/stored
/// scalar summaries, and whether the formula is retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormulaCacheWriteReceipt {
    /// Formula cell receiving the saved result.
    pub target: EvaluationTarget,
    /// Conversion policy and pinned source identity used for publication.
    pub identity: FormulaCacheMaterializationIdentity,
    /// Raw computed scalar summary.
    pub computed: MaterializedScalarSummary,
    /// Scalar summary after saved-result conversion.
    pub stored: MaterializedScalarSummary,
    /// Whether publication preserves the authored formula.
    pub formula_retained: bool,
}
