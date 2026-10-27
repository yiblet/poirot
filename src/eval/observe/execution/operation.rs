use super::operation_id::OperationId;
use super::operation_kind::OperationKind;
use crate::eval::coordinates::EvaluationTarget;

/// An entered expression or argument with its parent, target cell, and execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    /// Run-local operation identity.
    pub id: OperationId,
    /// Operation whose execution entered this one, if any.
    pub parent: Option<OperationId>,
    /// Execution order within the parent, distinct from an argument ordinal.
    pub child_sequence: u64,
    /// Formula cell executing the operation.
    pub target: EvaluationTarget,
    /// Expression or selected argument metadata.
    pub kind: OperationKind,
}
