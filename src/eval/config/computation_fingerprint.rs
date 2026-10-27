use super::array_result_policy::ArrayResultPolicy;
use super::builtin_semantics_policy::BuiltinSemanticsPolicy;
use super::producer_policy::ProducerPolicy;

/// Execution policies that affect raw computed values independently of consumed context.
/// Incremental cache validation compares this projection separately from the
/// formatting, text, clock, and localization settings each cell actually read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComputationFingerprint {
    /// Numeric result behavior affecting raw computation.

    /// Builtin compatibility behavior affecting raw computation.
    pub builtin_policy: BuiltinSemanticsPolicy,
    /// Array publication behavior affecting computed results.
    pub array_result_policy: ArrayResultPolicy,
    /// Producer computation or caller-trusted saved-source behavior.
    pub producer_policy: ProducerPolicy,
    /// Pinned numeric implementation identity, when selected.
    pub numeric_source: crate::eval::values::numeric_outcome::NumericSourceBinding,
}
