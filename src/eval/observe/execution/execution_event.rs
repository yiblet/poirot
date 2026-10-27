use super::observed_value::ObservedValue;
use super::operation::Operation;
use super::operation_id::OperationId;
use crate::eval::functions::registry::LogicalFunction;
use crate::eval::limits::EvaluationFailure;
use crate::model::{CellError, CellValue};

/// An observed begin, completion, or logical error-masking decision.
/// Events describe executed operations; matching errors alone do not prove propagation.
#[derive(Clone, Copy, Debug)]
pub enum ExecutionEvent<'a> {
    /// Operation was entered before evaluating its body.
    Begin(Operation),
    /// Operation finished with borrowed result evidence or an operational failure.
    Complete {
        /// Operation that finished.
        operation: Operation,
        /// Observed result channel or failure.
        result: Result<ObservedValue<'a>, &'a EvaluationFailure>,
    },
    /// The handler actually scalar-checked this source argument. A selected fallback
    /// is a later Argument operation with this handler parent and ordinal 1.
    /// Selection does not assert fallback completion or internal error propagation.
    MaskDecision {
        /// Logical handler operation making the decision.
        handler: OperationId,
        /// Logical builtin whose error rule was applied.
        function: LogicalFunction,
        /// Evaluated source argument operation.
        source_argument: OperationId,
        /// Scalar actually checked by the handler.
        checked: &'a CellValue,
        /// Spreadsheet error caught by the handler, if any.
        caught: Option<CellError>,
        /// Selected fallback argument position, if any.
        selected_fallback_ordinal: Option<usize>,
        /// Actual entered fallback operation, when present.
        selected_fallback_operation: Option<OperationId>,
    },
}
