//! Optional bounded retention of execution events.
//!
//! `BoundedExecutionTrace` is an `ExecutionObserver` that copies borrowed events into
//! owned records under explicit limits on logical records and text bytes, not total heap.
//!
//! Depends on: functions (logical handler identities) and limits (capabilities and
//! failures).

/// Execution observer that retains owned events under explicit limits.
mod bounded_execution_trace;
/// Owned copy of one execution event.
mod retained_event;
/// Owned scalar evidence or shape summary.
mod retained_value;
/// Limits on retained events, scalar values and text bytes.
mod retention_limits;

pub use bounded_execution_trace::BoundedExecutionTrace;
pub use retained_event::RetainedEvent;
pub use retained_value::RetainedValue;
pub use retention_limits::RetentionLimits;
