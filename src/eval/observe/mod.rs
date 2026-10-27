//! Execution observation, bounded event retention and consumed builtin calls.
//!
//! `execution` defines the borrowed operation events that the session's
//! `ObservationRecorder` emits, `retention` keeps owned copies under explicit limits, and
//! `consumed_calls` reports builtin handlers actually entered during a run.
//!
//! Depends on: functions (handler identities), coordinates (targets), limits (failures
//! and results) and values (value channels).

/// Caller-owned observation of actual builtin handler entry.
mod consumed_calls;
/// Borrowed execution events and the executor that records them.
mod execution;
/// Bounded owned retention of execution events.
mod retention;

#[cfg(test)]
pub(in crate::eval) use consumed_calls::builtin_call;
pub use consumed_calls::{BuiltinInvocationObserver, ConsumedBuiltinInvocation};
pub use execution::{
    ArgumentOrigin, ExecutionEvent, ExecutionObserver, ObservedValue, Operation, OperationId,
    OperationKind,
};
pub(in crate::eval) use execution::{ObservationState, observed_value};
pub use retention::{BoundedExecutionTrace, RetainedEvent, RetainedValue, RetentionLimits};
