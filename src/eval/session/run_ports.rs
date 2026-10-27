use crate::eval::limits::EvalResult;
use crate::eval::observe::{BuiltinInvocationObserver, ExecutionEvent, ExecutionObserver};
use crate::eval::ports::{CachedCellReader, CellObserver, DependencyReadSink};

/// Caller-installed ports that observe or supply values during one run.
#[derive(Default)]
pub(in crate::eval) struct RunPorts<'a> {
    /// Receives every consumed dependency read.
    pub(in crate::eval) dependency_sink: Option<&'a mut dyn DependencyReadSink>,
    /// Supplies cached cell results before evaluation.
    pub(in crate::eval) cell_reader: Option<&'a mut dyn CachedCellReader>,
    /// Receives completed cell evaluations.
    pub(in crate::eval) cell_observer: Option<&'a mut dyn CellObserver>,
    /// Admits or refuses builtin invocations before their effects.
    pub(in crate::eval) builtin_invocation_observer: Option<&'a mut dyn BuiltinInvocationObserver>,
    /// Receives optional tracing and profiling events.
    pub(in crate::eval) execution_observer: Option<&'a mut dyn ExecutionObserver>,
}

impl RunPorts<'_> {
    /// Forward an execution event to the installed observer, if any.
    pub(in crate::eval) fn observation_event(
        &mut self,
        event: ExecutionEvent<'_>,
    ) -> EvalResult<()> {
        match &mut self.execution_observer {
            Some(observer) => observer.observe(event),
            None => Ok(()),
        }
    }

    /// Offer a resolved builtin invocation to the installed observer, if any.
    pub(in crate::eval) fn observe_resolved_call(
        &mut self,
        invocation: Option<crate::eval::observe::ConsumedBuiltinInvocation<'_>>,
    ) -> EvalResult<()> {
        if let (Some(observer), Some(invocation)) =
            (&mut self.builtin_invocation_observer, invocation)
        {
            observer.consumed(invocation)?;
        }
        Ok(())
    }
}
