//! Execution events for tracing and profiling actual evaluation.
//!
//! Events describe actual execution only: identical errors do not establish
//! propagation, no source spans are invented, and uninstrumented coercions or
//! helpers are unattributed. The session's `ObservationRecorder` executor assigns
//! operation identities and delivers events to the run's `ExecutionObserver`.
//!
//! Depends on: functions (handler identities), coordinates (targets), limits (failures
//! and results) and values (value channels).

/// Kind of syntactic operand an evaluated argument came from.
mod argument_origin;
/// One borrowed begin, completion or error-masking event.
mod execution_event;
/// Caller-owned sink for execution events.
mod execution_observer;
/// Run-local identity counter and active parent operation.
mod observation_state;
/// Borrowed scalar or shape evidence of an operation result.
mod observed_value;
/// One entered expression or argument operation.
mod operation;
/// Run-local operation identity.
mod operation_id;
/// Whether an operation is an expression or a selected argument.
mod operation_kind;
/// Unit tests for execution observation through lookup, error handling and caps.
#[cfg(test)]
mod tests;
/// Summarizes evaluated values as observation evidence.
mod value_evidence;

pub use argument_origin::ArgumentOrigin;
pub use execution_event::ExecutionEvent;
pub use execution_observer::ExecutionObserver;
pub(in crate::eval) use observation_state::ObservationState;
pub use observed_value::ObservedValue;
pub use operation::Operation;
pub use operation_id::OperationId;
pub use operation_kind::OperationKind;
pub(in crate::eval) use value_evidence::observed_value;
