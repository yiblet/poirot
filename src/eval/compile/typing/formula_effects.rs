use super::external_behavior::ExternalBehavior;
use crate::eval::functions::registry::DependencyPolicy;

/// Conservative bounds on the workbook services and external behavior an expression may use.
/// Preparation combines these effects across operands; actual consumed-read traces
/// still determine whether a calculated result can be reused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FormulaEffects {
    /// Existing conservative host capability categories, not actual consumed reads.
    pub possible_reads: DependencyPolicy,
    /// External reuse contract, independent of captured clock or seed reads.
    pub external: ExternalBehavior,
}

impl FormulaEffects {
    /// Whether every possible dependency/effect is admitted by the other characteristics.
    pub fn is_subtype_of(self, other: Self) -> bool {
        other.possible_reads.contains(self.possible_reads) && self.external <= other.external
    }

    /// Least upper bound of possible reads and external behavior.
    pub fn join(self, other: Self) -> Self {
        Self {
            possible_reads: self.possible_reads.union(other.possible_reads),
            external: self.external.max(other.external),
        }
    }
}
