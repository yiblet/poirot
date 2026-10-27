use super::composition::Session;
use super::projection::ResultProjection;
#[cfg(test)]
use crate::eval::compile::bound::Expression;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::LogicalFunction;
use crate::eval::limits::{EvalResult, EvaluationFailure};
use crate::eval::observe::{
    ArgumentOrigin, ExecutionEvent, ObservedValue, Operation, OperationId, OperationKind,
    observed_value,
};
use crate::eval::values::Value;
use crate::eval::vm::{ArgumentsRef, ExecutableRef, ExpressionRunner};
use crate::model::{CellAddress, CellError, CellValue};

/// Emits execution observation events and maintains the active observation frame.
pub(in crate::eval) struct ObservationRecorder<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> ObservationRecorder<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Emit entry and install a frame only after the observer accepts it.
    pub(in crate::eval) fn observation_begin(
        &mut self,
        target: EvaluationTarget,
        kind: OperationKind,
    ) -> EvalResult<(Operation, Option<(Operation, u64)>)> {
        let next = self
            .session
            .state
            .runtime
            .observation_state
            .next
            .checked_add(1)
            .ok_or_else(|| EvaluationFailure::ResourceLimit {
                capability: crate::eval::limits::CapabilityId::ExecutionObservation,
                source: Some(target),
                detail: "execution operation identity exhausted".into(),
            })?;
        self.session.state.runtime.observation_state.next = next;
        let (parent, child_sequence) = match &mut self.session.state.runtime.observation_state.frame
        {
            Some((operation, children)) => {
                let ordinal = *children;
                *children += 1;
                (Some(operation.id), ordinal)
            }
            None => (None, 0),
        };
        let operation = Operation {
            id: OperationId(next),
            parent,
            child_sequence,
            target,
            kind,
        };
        // A rejected begin never becomes the active frame.
        self.session
            .ports
            .observation_event(ExecutionEvent::Begin(operation))?;
        let previous = self
            .session
            .state
            .runtime
            .observation_state
            .frame
            .replace((operation, 0));
        Ok((operation, previous))
    }

    /// Restore the parent frame and emit completion without replacing an existing failure.
    pub(in crate::eval) fn observation_end(
        &mut self,
        operation: Operation,
        previous: Option<(Operation, u64)>,
        result: Result<ObservedValue<'_>, &EvaluationFailure>,
    ) -> EvalResult<()> {
        self.session.state.runtime.observation_state.frame = previous;
        let notification = self
            .session
            .ports
            .observation_event(ExecutionEvent::Complete { operation, result });
        // An observer cannot replace an already existing operational failure.
        if result.is_ok() { notification } else { Ok(()) }
    }

    /// Execute a tree expression with optional begin/completion observations.
    #[cfg(test)]
    pub(in crate::eval) fn observed_expression(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: &Expression,
    ) -> EvalResult<Value> {
        if self.session.ports.execution_observer.is_none() {
            return ExpressionRunner::new(self.session).inner(sheet, at, expression);
        }
        let handler = ExpressionRunner::new(self.session)
            .builtin_invocation(expression)
            .map(|(handler, _)| handler);
        let (operation, previous) = self.observation_begin(
            EvaluationTarget { sheet, address: at },
            OperationKind::Expression { handler },
        )?;
        let result = ExpressionRunner::new(self.session).inner(sheet, at, expression);
        self.observation_end(operation, previous, result.as_ref().map(observed_value))?;
        result
    }

    /// Observe argument evaluation together with its scalar projection.
    pub(in crate::eval) fn observed_scalar_argument(
        &mut self,
        sheet: usize,
        at: CellAddress,
        args: ArgumentsRef<'_>,
        ordinal: usize,
    ) -> EvalResult<CellValue> {
        let expression = args.get(ordinal);
        let (operation, previous) = self.observation_begin(
            EvaluationTarget { sheet, address: at },
            OperationKind::Argument {
                ordinal,
                origin: expression.map_or(ArgumentOrigin::Missing, ExecutableRef::origin),
            },
        )?;
        let result = match expression {
            Some(expression) => ExpressionRunner::new(self.session)
                .expr_with_demand(
                    sheet,
                    at,
                    expression,
                    super::expression_demand::ExpressionDemand::Scalar,
                )
                .and_then(|v| ResultProjection::new(self.session).select_legacy_scalar(at, v)),
            None => Ok(CellValue::Blank),
        };
        self.observation_end(
            operation,
            previous,
            result.as_ref().map(ObservedValue::Scalar),
        )?;
        result
    }

    /// Observe a selected argument while preserving its reference or array channel.
    pub(in crate::eval) fn observed_value_argument(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: ExecutableRef<'_>,
        ordinal: usize,
    ) -> EvalResult<(Option<OperationId>, Value)> {
        if self.session.ports.execution_observer.is_none() {
            return ExpressionRunner::new(self.session)
                .expr(sheet, at, expression)
                .map(|value| (None, value));
        }
        let (operation, previous) = self.observation_begin(
            EvaluationTarget { sheet, address: at },
            OperationKind::Argument {
                ordinal,
                origin: expression.origin(),
            },
        )?;
        let result = ExpressionRunner::new(self.session).expr(sheet, at, expression);
        self.observation_end(operation, previous, result.as_ref().map(observed_value))?;
        result.map(|value| (Some(operation.id), value))
    }

    /// Enter and observe the fallback selected by a logical error handler.
    pub(in crate::eval) fn observed_fallback(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: ExecutableRef<'_>,
        function: LogicalFunction,
        source: Option<OperationId>,
        checked: &CellValue,
    ) -> EvalResult<Value> {
        if self.session.ports.execution_observer.is_none() {
            return ExpressionRunner::new(self.session).expr(sheet, at, expression);
        }
        let caught = match checked {
            CellValue::Error(error) => Some(*error),
            _ => None,
        };
        let (operation, previous) = self.observation_begin(
            EvaluationTarget { sheet, address: at },
            OperationKind::Argument {
                ordinal: 1,
                origin: expression.origin(),
            },
        )?;
        // The allocated fallback operation is real; no future identity is guessed.
        // Temporarily use its handler parent for the decision, restoring on all exits.
        let fallback_frame = self.session.state.runtime.observation_state.frame;
        self.session.state.runtime.observation_state.frame = previous;
        let decision = self.observe_mask(function, source, checked, caught, Some(operation.id));
        self.session.state.runtime.observation_state.frame = fallback_frame;
        let result =
            decision.and_then(|()| ExpressionRunner::new(self.session).expr(sheet, at, expression));
        self.observation_end(operation, previous, result.as_ref().map(observed_value))?;
        result
    }

    /// Report an actual scalar error check and its fallback selection.
    pub(in crate::eval) fn observe_mask(
        &mut self,
        function: LogicalFunction,
        source: Option<OperationId>,
        checked: &CellValue,
        caught: Option<CellError>,
        selected_fallback_operation: Option<OperationId>,
    ) -> EvalResult<()> {
        if let (Some(source_argument), Some((operation, _))) =
            (source, self.session.state.runtime.observation_state.frame)
        {
            self.session
                .ports
                .observation_event(ExecutionEvent::MaskDecision {
                    handler: operation.id,
                    function,
                    source_argument,
                    checked,
                    caught,
                    selected_fallback_ordinal: caught.map(|_| 1),
                    selected_fallback_operation,
                })?;
        }
        Ok(())
    }
}
