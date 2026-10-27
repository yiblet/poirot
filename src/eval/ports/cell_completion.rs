use super::cell_provenance::CellProvenance;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationError;
use crate::model::CellValue;

/// A borrowed observation after each recursive cell call resolves, including
/// semantic errors and operational aborts. No result copy is retained by the engine.
#[derive(Debug)]
pub struct CellCompletion<'a> {
    /// Formula that requested this cell, when available.
    pub consumer: Option<EvaluationTarget>,
    /// Cell whose call completed.
    pub target: EvaluationTarget,
    /// Computed, session-memo, or external-cache origin on success.
    /// On operational failure this identifies the attempted resolution route,
    /// not proof that a formula body ran; RunUsage::formula_executions counts bodies.
    pub provenance: CellProvenance,
    /// Borrowed value or operational failure from the call.
    pub result: Result<&'a CellValue, &'a EvaluationError>,
}
