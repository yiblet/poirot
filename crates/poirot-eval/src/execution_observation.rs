//! Actual execution only. Identical errors do not establish propagation.
//! No source spans are invented; uninstrumented coercions/helpers are unattributed.
use crate::{
    EvaluationFailure, EvaluationTarget,
    registry::{FunctionId, LogicalFunction},
};
use poirot_model::{CellError, CellValue};

/// Run-local identifier linking an observed operation to its parent and completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationId(
    /// Monotonically assigned identity within one run.
    pub u64,
);
/// An expression evaluation or one selected argument operation in the execution trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationKind {
    /// Evaluation of an expression node.
    Expression {
        /// Builtin handler, when the expression is a resolved builtin call.
        handler: Option<FunctionId>,
    },
    /// Evaluation of one selected call argument.
    Argument {
        /// Zero-based syntactic argument position.
        ordinal: usize,
        /// Operand provenance before scalar projection.
        origin: ArgumentOrigin,
    },
}
/// Syntactic/runtime origin of an evaluated argument before value projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgumentOrigin {
    /// Omitted syntactic operand.
    Missing,
    /// Literal expression.
    Literal,
    /// Reference expression.
    Reference,
    /// Expression requiring computation.
    Computed,
}
/// An entered expression or argument with its parent, target cell, and execution order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    /// Run-local operation identity.
    pub id: OperationId,
    /// Operation whose execution entered this one, if any.
    pub parent: Option<OperationId>,
    /// Execution order within the parent, distinct from an argument ordinal.
    pub child_sequence: u64,
    /// Formula cell executing the operation.
    pub target: EvaluationTarget,
    /// Expression or selected argument metadata.
    pub kind: OperationKind,
}
/// Borrowed result evidence: scalar data, omitted syntax, or array/reference shape.
/// Shape evidence does not expose every array element or reference operand.
#[derive(Clone, Copy, Debug)]
pub enum ObservedValue<'a> {
    /// Borrowed scalar evidence, including spreadsheet errors.
    Scalar(&'a CellValue),
    /// Omitted-argument evidence.
    Missing,
    /// Shape summary without copying array elements.
    Array {
        /// Number of materialized rows.
        rows: usize,
        /// Total number of materialized elements.
        values: usize,
    },
    /// Reference shape without reading its cells.
    Reference {
        /// Number of reference areas.
        areas: usize,
        /// Whether this is a true multi-sheet reference.
        sheet_span: bool,
    },
}
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
/// Caller-owned sink for borrowed execution events.
/// An observer failure aborts evaluation. Admit storage before copying payloads and
/// retain earlier events as a partial trace when the run fails.
pub trait ExecutionObserver {
    /// Admit event/value/text capacity before cloning any borrowed payload.
    /// Retained evidence is partial after any callback or evaluator failure.
    fn observe(&mut self, event: ExecutionEvent<'_>) -> Result<(), EvaluationFailure>;
}
/// Run-local identity counter and active observation frame.
#[derive(Default)]
pub(crate) struct ObservationState {
    next: u64,
    frame: Option<(Operation, u64)>,
}

use crate::{ArgumentsRef, ExecutableRef, Session, Value, budget::EvalResult};
use poirot_model::CellAddress;
impl Session<'_> {
    fn observation_event(&mut self, event: ExecutionEvent<'_>) -> EvalResult<()> {
        match &mut self.execution_observer {
            Some(observer) => observer.observe(event),
            None => Ok(()),
        }
    }
    /// Emit entry and install a frame only after the observer accepts it.
    pub(super) fn observation_begin(
        &mut self,
        target: EvaluationTarget,
        kind: OperationKind,
    ) -> EvalResult<(Operation, Option<(Operation, u64)>)> {
        let next = self.observation_state.next.checked_add(1).ok_or_else(|| {
            EvaluationFailure::ResourceLimit {
                capability: crate::CapabilityId::FunctionAdapter,
                source: Some(target),
                detail: "execution operation identity exhausted".into(),
            }
        })?;
        self.observation_state.next = next;
        let (parent, child_sequence) = match &mut self.observation_state.frame {
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
        self.observation_event(ExecutionEvent::Begin(operation))?;
        let previous = self.observation_state.frame.replace((operation, 0));
        Ok((operation, previous))
    }
    /// Restore the parent frame and emit completion without replacing an existing failure.
    pub(super) fn observation_end(
        &mut self,
        operation: Operation,
        previous: Option<(Operation, u64)>,
        result: Result<ObservedValue<'_>, &EvaluationFailure>,
    ) -> EvalResult<()> {
        self.observation_state.frame = previous;
        let notification = self.observation_event(ExecutionEvent::Complete { operation, result });
        // An observer cannot replace an already existing operational failure.
        if result.is_ok() { notification } else { Ok(()) }
    }
    /// Execute a tree expression with optional begin/completion observations.
    #[cfg(test)]
    pub(super) fn observed_expression(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: &Expr,
    ) -> EvalResult<Value> {
        if self.execution_observer.is_none() {
            return self.inner(sheet, at, expression);
        }
        let handler = self
            .builtin_invocation(expression)
            .map(|(handler, _)| handler);
        let (operation, previous) = self.observation_begin(
            EvaluationTarget { sheet, address: at },
            OperationKind::Expression { handler },
        )?;
        let result = self.inner(sheet, at, expression);
        self.observation_end(operation, previous, result.as_ref().map(observed_value))?;
        result
    }
    /// Observe argument evaluation together with its scalar projection.
    pub(super) fn observed_scalar_argument(
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
            Some(expression) => self
                .expr_with_demand(
                    sheet,
                    at,
                    expression,
                    crate::expression_execution::ExpressionDemand::Scalar,
                )
                .and_then(|v| self.scalar(at, v)),
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
    pub(super) fn observed_value_argument(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: ExecutableRef<'_>,
        ordinal: usize,
    ) -> EvalResult<(Option<OperationId>, Value)> {
        if self.execution_observer.is_none() {
            return self.expr(sheet, at, expression).map(|value| (None, value));
        }
        let (operation, previous) = self.observation_begin(
            EvaluationTarget { sheet, address: at },
            OperationKind::Argument {
                ordinal,
                origin: expression.origin(),
            },
        )?;
        let result = self.expr(sheet, at, expression);
        self.observation_end(operation, previous, result.as_ref().map(observed_value))?;
        result.map(|value| (Some(operation.id), value))
    }
    /// Enter and observe the fallback selected by a logical error handler.
    pub(super) fn observed_fallback(
        &mut self,
        sheet: usize,
        at: CellAddress,
        expression: ExecutableRef<'_>,
        function: LogicalFunction,
        source: Option<OperationId>,
        checked: &CellValue,
    ) -> EvalResult<Value> {
        if self.execution_observer.is_none() {
            return self.expr(sheet, at, expression);
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
        let fallback_frame = self.observation_state.frame;
        self.observation_state.frame = previous;
        let decision = self.observe_mask(function, source, checked, caught, Some(operation.id));
        self.observation_state.frame = fallback_frame;
        let result = decision.and_then(|()| self.expr(sheet, at, expression));
        self.observation_end(operation, previous, result.as_ref().map(observed_value))?;
        result
    }
    /// Report an actual scalar error check and its fallback selection.
    pub(super) fn observe_mask(
        &mut self,
        function: LogicalFunction,
        source: Option<OperationId>,
        checked: &CellValue,
        caught: Option<CellError>,
        selected_fallback_operation: Option<OperationId>,
    ) -> EvalResult<()> {
        if let (Some(source_argument), Some((operation, _))) =
            (source, self.observation_state.frame)
        {
            self.observation_event(ExecutionEvent::MaskDecision {
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
/// Borrow scalar evidence or summarize the shape of a nonscalar value.
pub(super) fn observed_value(value: &Value) -> ObservedValue<'_> {
    match value {
        Value::Scalar(value) | Value::SelectedScalar(value, _) => ObservedValue::Scalar(value),
        Value::Missing => ObservedValue::Missing,
        Value::Array(array) => ObservedValue::Array {
            rows: array.values.len(),
            values: array.values.iter().map(Vec::len).sum(),
        },
        Value::Refs(areas) => ObservedValue::Reference {
            areas: areas.len(),
            sheet_span: false,
        },
        Value::SignedRefs(areas) => ObservedValue::Reference {
            areas: areas.len(),
            sheet_span: false,
        },
        Value::SheetSpan(areas) => ObservedValue::Reference {
            areas: areas.len(),
            sheet_span: true,
        },
    }
}

#[cfg(test)]
use crate::bound::Expression as Expr;
