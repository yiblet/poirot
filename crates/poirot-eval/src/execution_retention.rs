//! Optional bounded retention adapter; logical records/text bytes, not total heap.
use crate::execution_observation::*;
use crate::{CapabilityId, EvaluationFailure, registry::LogicalFunction};
use poirot_model::{CellError, CellValue};
/// Limits on event count, copied scalar count, and physical text bytes in an execution trace.
#[derive(Clone, Copy, Debug)]
pub struct RetentionLimits {
    /// Maximum retained execution events.
    pub events: usize,
    /// Maximum scalar evidence values copied into the trace.
    pub scalar_values: usize,
    /// Maximum physical text bytes copied into retained evidence.
    pub text_bytes: usize,
}
/// Owned scalar evidence or an array/reference shape summary copied from an observation.
#[derive(Clone, Debug)]
pub enum RetainedValue {
    /// Owned scalar evidence.
    Scalar(CellValue),
    /// Omitted-argument evidence.
    Missing,
    /// Array shape without retained elements.
    Array {
        /// Number of rows.
        rows: usize,
        /// Total element count.
        values: usize,
    },
    /// Reference shape without retained cell contents.
    Reference {
        /// Number of reference areas.
        areas: usize,
        /// Whether the reference spans sheets.
        sheet_span: bool,
    },
}
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
/// An execution observer that owns evidence under explicit retention limits.
/// Capacity refusal aborts the run. Call [`BoundedExecutionTrace::finish`] even when
/// failure occurred before the first event, so partial status reflects the run outcome.
pub struct BoundedExecutionTrace {
    limits: RetentionLimits,
    events: Vec<RetainedEvent>,
    values: usize,
    text: usize,
    partial: bool,
    rejected: bool,
}
impl BoundedExecutionTrace {
    /// Create an empty trace under explicit retention limits.
    pub fn new(limits: RetentionLimits) -> Self {
        Self {
            limits,
            events: Vec::new(),
            values: 0,
            text: 0,
            partial: false,
            rejected: false,
        }
    }
    /// Borrow retained events in execution order.
    pub fn events(&self) -> &[RetainedEvent] {
        &self.events
    }
    /// Whether evaluation or evidence retention stopped before a complete trace.
    pub fn is_partial(&self) -> bool {
        self.partial
    }
    /// Required after the run even when failure precedes the first execution event.
    pub fn finish(&mut self, result: Result<(), &EvaluationFailure>) {
        self.partial |= result.is_err();
    }
    fn failure(&mut self) -> EvaluationFailure {
        self.partial = true;
        self.rejected = true;
        EvaluationFailure::ResourceLimit {
            capability: CapabilityId::FunctionAdapter,
            source: None,
            detail: "execution evidence retention capacity exhausted".into(),
        }
    }
}
fn text_bytes(value: &CellValue) -> usize {
    value.text_ref().map_or(0, |text| text.storage_bytes())
}
fn failure_text(failure: &EvaluationFailure) -> usize {
    match failure {
        EvaluationFailure::ResourceLimit { detail, .. }
        | EvaluationFailure::UnsupportedCapability { detail, .. } => detail.len(),
        _ => 0,
    }
}
impl ExecutionObserver for BoundedExecutionTrace {
    fn observe(&mut self, event: ExecutionEvent<'_>) -> Result<(), EvaluationFailure> {
        let (values, text) = match event {
            ExecutionEvent::Complete {
                result: Ok(ObservedValue::Scalar(value)),
                ..
            } => (1, text_bytes(value)),
            ExecutionEvent::Complete {
                result: Err(failure),
                ..
            } => {
                self.partial = true;
                (0, failure_text(failure))
            }
            ExecutionEvent::MaskDecision { checked, .. } => (1, text_bytes(checked)),
            _ => (0, 0),
        };
        let next_values = self.values.checked_add(values);
        let next_text = self.text.checked_add(text);
        if self.rejected
            || self.events.len() >= self.limits.events
            || next_values.is_none_or(|n| n > self.limits.scalar_values)
            || next_text.is_none_or(|n| n > self.limits.text_bytes)
        {
            return Err(self.failure());
        }
        // Every retained clone below follows all cumulative admissions.
        let retained = match event {
            ExecutionEvent::Begin(operation) => RetainedEvent::Begin(operation),
            ExecutionEvent::Complete { operation, result } => RetainedEvent::Complete {
                operation,
                result: result
                    .map(|value| match value {
                        ObservedValue::Scalar(value) => RetainedValue::Scalar(value.clone()),
                        ObservedValue::Missing => RetainedValue::Missing,
                        ObservedValue::Array { rows, values } => {
                            RetainedValue::Array { rows, values }
                        }
                        ObservedValue::Reference { areas, sheet_span } => {
                            RetainedValue::Reference { areas, sheet_span }
                        }
                    })
                    .map_err(Clone::clone),
            },
            ExecutionEvent::MaskDecision {
                handler,
                function,
                source_argument,
                checked,
                caught,
                selected_fallback_operation,
                ..
            } => RetainedEvent::MaskDecision {
                handler,
                function,
                source_argument,
                checked: checked.clone(),
                caught,
                selected_fallback_operation,
            },
        };
        // SAFETY: The admission guard rejects None from either checked addition before these values are unwrapped.
        self.values = next_values.unwrap();
        // SAFETY: The admission guard rejects None from either checked addition before these values are unwrapped.
        self.text = next_text.unwrap();
        self.events.push(retained);
        Ok(())
    }
}
