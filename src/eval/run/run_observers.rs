use crate::eval::ports::CellObserver;
use crate::eval::ports::DependencyReadSink;

/// Caller-supplied observers attached to the run's execution session.
/// They receive dependency reads, cell completions, builtin entries, and execution
/// operations from the same calculation without a separate evaluation pass.
pub struct RunObservers<'a> {
    /// Sink for consumed dependency reads.
    pub reads: &'a mut dyn DependencyReadSink,
    /// Sink for completed cell values and provenance.
    pub cells: &'a mut dyn CellObserver,
    /// Sink for actual builtin invocations.
    pub builtins: &'a mut dyn crate::eval::observe::BuiltinInvocationObserver,
    /// Sink for execution operations and reuse events.
    pub operations: &'a mut dyn crate::eval::observe::ExecutionObserver,
}
