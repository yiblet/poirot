use super::budget_error::BudgetError;
use super::capability_id::CapabilityId;
use super::resource_failure_kind::ResourceFailureKind;
use crate::eval::coordinates::EvaluationTarget;

/// A failure that stops calculation rather than producing a spreadsheet error value.
///
/// Includes exhausted limits, stale plans, and unsupported operations. Formula
/// error handlers such as `IFERROR` cannot catch these failures.
#[derive(Clone, Debug, thiserror::Error)]
pub enum EvaluationError {
    /// A calculated value would exceed the session's cached-result storage limit.
    #[error(
        "memo retention limit {limits:?} exceeded (consumed {consumed:?}, requested {requested:?})"
    )]
    MemoRetentionLimit {
        /// Configured cell and byte retention limits.
        limits: crate::eval::limits::memo_retention_limits::MemoRetentionLimits,
        /// Retention usage before the rejected admission.
        consumed: crate::eval::limits::memo_retention_usage::MemoRetentionUsage,
        /// Additional retention requested.
        requested: crate::eval::limits::memo_retention_usage::MemoRetentionUsage,
        /// Formula active when admission failed.
        target: Option<EvaluationTarget>,
    },
    /// Formula/name/sheet topology or preparation policy differs from the plan.
    #[error("evaluation plan source or policy is stale")]
    StalePlan,
    /// A metered engine operation exceeded its configured limit.
    #[error("evaluation budget exceeded")]
    Budget(#[from] BudgetError),
    /// A representation, allocation, or backend failure with no invented usage counts.
    #[error("evaluation {kind:?} in {capability:?}")]
    Resource {
        /// Actual failure category, independent of application quotas.
        kind: ResourceFailureKind,
        /// Rejecting subsystem.
        capability: CapabilityId,
        /// Active cell when known.
        target: Option<EvaluationTarget>,
        /// Operation or native backend diagnostic.
        #[source]
        reason: crate::eval::limits::budget::evaluation_reason_error::EvaluationReasonError,
    },
    /// A component exceeded a structural limit or overflowed a size estimate.
    ///
    /// Unlike [`Self::Budget`], this has no evaluator work charge to report.
    #[error("evaluation resource limit in {capability:?}")]
    ResourceLimit {
        /// Capability whose internal resource limit was exceeded.
        capability: CapabilityId,
        /// Cell executing the operation, when available.
        target: Option<EvaluationTarget>,
        /// Explanation supplied by the rejecting component.
        #[source]
        reason: crate::eval::limits::budget::evaluation_reason_error::EvaluationReasonError,
    },
    /// Execution requested behavior outside an installed capability's contract.
    #[error("unsupported evaluation capability {capability:?}")]
    UnsupportedCapability {
        /// Capability required by the operation.
        capability: CapabilityId,
        /// Cell executing the operation, when available.
        target: Option<EvaluationTarget>,
        /// Explanation of the unsupported behavior.
        #[source]
        reason: crate::eval::limits::budget::evaluation_reason_error::EvaluationReasonError,
    },
}

impl EvaluationError {
    /// Return metered budget details when this failure is a budget rejection.
    pub fn budget(&self) -> Option<&BudgetError> {
        match self {
            Self::Budget(failure) => Some(failure),
            Self::UnsupportedCapability { .. }
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
            Self::UnsupportedCapability { target: source, .. }
            | Self::ResourceLimit { target: source, .. }
            | Self::Resource { target: source, .. }
            | Self::MemoRetentionLimit { target: source, .. } => *source = Some(target),
            Self::StalePlan => {}
        }
    }

    /// Attribute a direct storage reservation while retaining its allocator cause.
    pub(in crate::eval) fn reservation(
        capability: CapabilityId,
        target: Option<EvaluationTarget>,
        operation: super::evaluation_reason_error::EvaluationReasonError,
        source: std::collections::TryReserveError,
    ) -> Self {
        Self::Resource {
            kind: ResourceFailureKind::Allocation,
            capability,
            target,
            reason: operation.with_reservation_source(source),
        }
    }
}
