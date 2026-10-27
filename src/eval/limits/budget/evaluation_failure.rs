use super::budget_failure::BudgetFailure;
use super::capability_id::CapabilityId;
use super::resource_failure_kind::ResourceFailureKind;
use crate::eval::coordinates::EvaluationTarget;

/// A failure that stops calculation rather than producing a spreadsheet error value.
///
/// Includes exhausted limits, stale plans, and unsupported operations. Formula
/// error handlers such as `IFERROR` cannot catch these failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluationFailure {
    /// Setter publication requires the independently selected pinned computation policy.
    IncompatibleMaterializationPolicy,
    /// An upstream algorithm exception that stops calculation.
    KnownSourceException {
        /// Cell executing the exceptional source behavior.
        source: Option<EvaluationTarget>,
        /// Pinned source exception identity.
        exception: crate::model::KnownSourceException,
    },
    /// A calculated value would exceed the session's cached-result storage limit.
    MemoRetentionLimit {
        /// Configured cell and byte retention limits.
        limits: crate::eval::limits::memo_retention_limits::MemoRetentionLimits,
        /// Retention usage before the rejected admission.
        consumed: crate::eval::limits::memo_retention_usage::MemoRetentionUsage,
        /// Additional retention requested.
        requested: crate::eval::limits::memo_retention_usage::MemoRetentionUsage,
        /// Formula active when admission failed.
        source: Option<EvaluationTarget>,
    },
    /// Formula/name/sheet topology or preparation policy differs from the plan.
    StalePlan,
    /// A metered engine operation exceeded its configured limit.
    Budget(BudgetFailure),
    /// A representation, allocation, or backend failure with no invented usage counts.
    Resource {
        /// Actual failure category, independent of application quotas.
        kind: ResourceFailureKind,
        /// Rejecting subsystem.
        capability: CapabilityId,
        /// Active cell when known.
        source: Option<EvaluationTarget>,
        /// Operation or native backend diagnostic.
        detail: String,
    },
    /// A component exceeded a structural limit or overflowed a size estimate.
    ///
    /// Unlike [`Self::Budget`], this has no evaluator work charge to report.
    ResourceLimit {
        /// Capability whose internal resource limit was exceeded.
        capability: CapabilityId,
        /// Cell executing the operation, when available.
        source: Option<EvaluationTarget>,
        /// Explanation supplied by the rejecting component.
        detail: String,
    },
    /// Execution requested behavior outside an installed capability's contract.
    UnsupportedCapability {
        /// Capability required by the operation.
        capability: CapabilityId,
        /// Cell executing the operation, when available.
        source: Option<EvaluationTarget>,
        /// Explanation of the unsupported behavior.
        detail: String,
    },
}

impl EvaluationFailure {
    /// Return metered budget details when this failure is a budget rejection.
    pub fn budget(&self) -> Option<&BudgetFailure> {
        match self {
            Self::Budget(failure) => Some(failure),
            Self::IncompatibleMaterializationPolicy
            | Self::KnownSourceException { .. }
            | Self::UnsupportedCapability { .. }
            | Self::ResourceLimit { .. }
            | Self::Resource { .. }
            | Self::StalePlan
            | Self::MemoRetentionLimit { .. } => None,
        }
    }

    /// Attach the active formula's coordinates to failures that support attribution.
    pub(in crate::eval) fn set_target(&mut self, target: EvaluationTarget) {
        match self {
            Self::Budget(failure) => failure.target = Some(target),
            Self::KnownSourceException { source, .. }
            | Self::UnsupportedCapability { source, .. }
            | Self::ResourceLimit { source, .. }
            | Self::Resource { source, .. }
            | Self::MemoRetentionLimit { source, .. } => *source = Some(target),
            Self::IncompatibleMaterializationPolicy | Self::StalePlan => {}
        }
    }
}

impl std::fmt::Display for EvaluationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleMaterializationPolicy => {
                f.write_str("pinned cell-setter materialization requires pinned numeric outcomes")
            }
            Self::KnownSourceException { exception, .. } => {
                write!(f, "known pinned source exception: {exception:?}")
            }
            Self::MemoRetentionLimit {
                limits,
                consumed,
                requested,
                ..
            } => write!(
                f,
                "memo retention limit {limits:?} exceeded (consumed {consumed:?}, requested {requested:?})"
            ),
            Self::StalePlan => f.write_str("evaluation plan source or policy is stale"),
            Self::Budget(failure) => write!(
                f,
                "evaluation {:?} limit {} exceeded (consumed {}, requested {})",
                failure.kind, failure.limit, failure.consumed, failure.requested
            ),
            Self::Resource {
                kind,
                capability,
                detail,
                ..
            } => write!(f, "evaluation {kind:?} in {capability:?}: {detail}"),
            Self::ResourceLimit {
                capability, detail, ..
            } => {
                write!(f, "evaluation resource limit in {capability:?}: {detail}")
            }
            Self::UnsupportedCapability {
                capability, detail, ..
            } => write!(
                f,
                "unsupported evaluation capability {capability:?}: {detail}"
            ),
        }
    }
}

impl std::error::Error for EvaluationFailure {}
