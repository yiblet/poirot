use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationFailure;
use crate::eval::ports::CellProvenance;
use crate::model::CellValue;

/// Owned cell completion withheld until actual ownership has settled.
pub(super) struct Completion {
    pub(super) consumer: Option<EvaluationTarget>,
    pub(super) target: EvaluationTarget,
    pub(super) provenance: CellProvenance,
    pub(super) result: Result<CellValue, EvaluationFailure>,
}
