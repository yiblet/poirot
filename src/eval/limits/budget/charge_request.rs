use super::work_kind::WorkKind;

/// Proposed work charge, committed by the meter only after admission.
#[derive(Clone, Copy)]
pub(super) struct ChargeRequest {
    /// Usage counter category.
    pub kind: WorkKind,
    /// Additional work units requested.
    pub amount: u64,
}
