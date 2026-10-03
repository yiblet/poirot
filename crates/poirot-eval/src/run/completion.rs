//! Borrowed completion observations and immutable external-cache handles.
use crate::{EvaluationFailure, EvaluationTarget};
use poirot_model::CellValue;
use std::sync::Arc;

/// Supplies validated results from a cache shared across evaluation runs.
/// Implementations must validate source, context, policy and dependency identities
/// before returning a completed value. `None` requests ordinary evaluation.
/// Return a cloned immutable handle: the engine checks text capacity before copying
/// its contents. Operational failures abort evaluation rather than becoming cell errors.
pub trait CachedCellReader {
    /// Return a validated immutable result, or request evaluation with `None`.
    fn cached_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<Option<Arc<CellValue>>, EvaluationFailure>;
}
/// Origin of a completed cell value within the current run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellProvenance {
    /// Computed from effective source content in this run.
    Evaluated,
    /// Reused from this run's memoized values.
    SessionMemo,
    /// Reused from a validated cross-run cache.
    ExternalCache,
}
/// A borrowed observation after each recursive cell call resolves, including
/// semantic errors and operational aborts. No result copy is retained by the engine.
#[derive(Debug)]
pub struct CellCompletion<'a> {
    /// Formula that requested this cell, when available.
    pub consumer: Option<EvaluationTarget>,
    /// Cell whose call completed.
    pub target: EvaluationTarget,
    /// Computed, session-memo, or external-cache origin.
    pub provenance: CellProvenance,
    /// Borrowed value or operational failure from the call.
    pub result: Result<&'a CellValue, &'a EvaluationFailure>,
}
/// The owner stages bounded version/cache updates here and commits only after the
/// entire run completes. An observer's capacity failure aborts the run; it must
/// discard its transaction on any abort, including failures after earlier cells.
pub trait CellObserver {
    /// Observe a completion; rejection aborts execution operationally.
    fn completed(&mut self, completion: CellCompletion<'_>) -> Result<(), EvaluationFailure>;
}
/// Result identity for incremental consumers. Spreadsheet comparisons intentionally
/// have different semantics: signed zero and NaN payloads remain distinct here.
pub fn same_result_identity(left: &CellValue, right: &CellValue) -> bool {
    match (left, right) {
        (CellValue::Number(left), CellValue::Number(right)) => left.to_bits() == right.to_bits(),
        _ => left == right,
    }
}
