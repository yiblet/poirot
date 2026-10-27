use super::execution_event::ExecutionEvent;
use crate::eval::limits::EvaluationError;

/// Caller-owned sink for borrowed execution events.
/// An observer failure aborts evaluation. Admit storage before copying payloads and
/// retain earlier events as a partial trace when the run fails.
pub trait ExecutionObserver {
    /// Admit event/value/text capacity before cloning any borrowed payload.
    /// Retained evidence is partial after any callback or evaluator failure.
    fn observe(&mut self, event: ExecutionEvent<'_>) -> Result<(), EvaluationError>;
}
