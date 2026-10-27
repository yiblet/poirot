use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationError;
use crate::model::CellValue;
use std::sync::Arc;

/// Supplies validated results from a cache shared across evaluation runs.
/// Implementations must validate source, context, policy and dependency identities
/// before returning a completed value. `None` requests ordinary evaluation.
/// Return a cloned immutable handle: the engine checks text capacity before copying
/// its contents. Operational failures abort evaluation rather than becoming cell errors.
/// Returned values must already obey worksheet publication semantics: a completed
/// formula returning blank is numeric zero. Physical empty cells remain blank.
/// The evaluator trusts this completed-result cache and does not normalize reads.
pub trait CachedCellReader {
    /// Return a validated immutable result, or request evaluation with `None`.
    fn cached_cell(
        &mut self,
        target: EvaluationTarget,
    ) -> Result<Option<Arc<CellValue>>, EvaluationError>;
}
