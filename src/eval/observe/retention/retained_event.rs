use super::retained_value::RetainedValue;
use crate::eval::functions::registry::LogicalFunction;
use crate::eval::limits::EvaluationFailure;
use crate::eval::observe::execution::{Operation, OperationId};
use crate::model::{CellError, CellValue};

/// Owned execution event retained after the evaluator's borrowed callback returns.
#[derive(Clone, Debug)]
pub enum RetainedEvent {
    /// Entered operation.
    Begin(Operation),
    /// Completed operation and copied result evidence.
    Complete {
        /// Operation that finished.
        operation: Operation,
        /// Owned evidence or operational failure.
        result: Result<RetainedValue, EvaluationFailure>,
    },
    /// Logical handler's actual error check and fallback selection.
    MaskDecision {
        /// Handler operation making the decision.
        handler: OperationId,
        /// Logical builtin whose rule was applied.
        function: LogicalFunction,
        /// Source argument operation that was checked.
        source_argument: OperationId,
        /// Copied scalar checked by the handler.
        checked: CellValue,
        /// Error caught by the handler, if any.
        caught: Option<CellError>,
        /// Actual fallback operation entered, if any.
        selected_fallback_operation: Option<OperationId>,
    },
}
