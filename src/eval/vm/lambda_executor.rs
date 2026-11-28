use super::arguments_ref::ArgumentsRef;
use super::bytecode_vm::Vm;
use super::expression_runner::ExpressionRunner;
use super::lambda_value::LambdaValue;
use super::local_frame::LocalFrame;
use crate::eval::compile::bytecode::{Compiled, Instruction, OperandCursor, ReferenceBindings};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{
    CapabilityId, EvalResult, EvaluationError, EvaluationReasonError, WorkKind,
};
use crate::eval::ports::DependencyRead;
use crate::eval::session::{NameResolver, ReadRecorder, Session};
use crate::eval::values::{FunctionError, FunctionResult, Value, copy_value, err};
use crate::model::{CellError, CellValue};
use std::sync::Arc;

// The default budget's expression depth also counts ordinary operators and deferred
// argument blocks. Limit callable nesting separately to bound named recursion.
const MAX_CALLABLE_DEPTH: usize = 16;

/// Creates closures and executes their bodies through ordinary VM frames.
pub(super) struct LambdaExecutor<'s, 'a> {
    session: &'s mut Session<'a>,
    target: EvaluationTarget,
}

impl<'s, 'a> LambdaExecutor<'s, 'a> {
    /// Borrow the session and bind resource attribution to the concrete formula cell.
    pub(super) fn new(session: &'s mut Session<'a>, target: EvaluationTarget) -> Self {
        Self { session, target }
    }

    /// Decode a closure descriptor, capturing only its selected source slots.
    pub(super) fn construct(
        &mut self,
        program: &Compiled,
        entry: usize,
        instruction: Instruction,
        frame: LocalFrame,
    ) -> EvalResult<Value> {
        let mut operands = OperandCursor(
            program
                .template
                .operands
                .get(instruction.a as usize..)
                .ok_or(EvaluationError::StalePlan)?,
        );
        let count = operands.word()? as usize;
        let arity = instruction.b as usize;
        if arity > 253 || count.saturating_add(arity) > program.template.local_slots as usize {
            return Err(EvaluationError::StalePlan);
        }
        let runtime = &mut *self.session.state.runtime;
        runtime.budget.allocation(count.max(1))?;
        let mut captures = Vec::new();
        reserve(&mut captures, count, self.target)?;
        for _ in 0..count {
            let slot = operands.word()?;
            let value = runtime
                .vm
                .locals
                .get(frame.index(slot)?)
                .ok_or(EvaluationError::StalePlan)?;
            let value = copy_value(&mut runtime.budget, value)?;
            // SAFETY: One capture is inserted for each slot in the successful reservation.
            captures.push(value);
        }

        let minimum_arity = operands.word()? as usize;
        if minimum_arity > arity {
            return Err(EvaluationError::StalePlan);
        }

        // Retain executable instructions by Arc and copy only instance coordinates.
        // Ordinary formula compilation keeps its existing reference ownership.
        runtime.budget.allocation(program.references.slots.len())?;
        runtime.budget.text(program.references.data.len())?;
        let mut slots = Vec::new();
        reserve(&mut slots, program.references.slots.len(), self.target)?;
        let mut data = Vec::new();
        reserve(&mut data, program.references.data.len(), self.target)?;
        // SAFETY: The reservations admit these exact source slice lengths.
        slots.extend_from_slice(&program.references.slots);
        data.extend_from_slice(&program.references.data);
        Ok(Value::Callable(Arc::new(LambdaValue {
            program: Compiled {
                executable: Arc::clone(&program.executable),
                references: ReferenceBindings { slots, data },
            },
            entry,
            arity,
            minimum_arity,
            captures,
        })))
    }

    /// Resolve a scoped defined name and invoke it using the caller's argument blocks.
    pub(super) fn invoke_named(
        &mut self,
        name: &str,
        arguments: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        let EvaluationTarget { sheet, address } = self.target;
        if let Some(callee) =
            NameResolver::new(self.session).lookup_defined_name(sheet, sheet, address, name)?
        {
            return self.invoke(callee, arguments);
        }
        ReadRecorder::new(self.session)
            .read_lazy(|| DependencyRead::OpaqueProvider(name.to_owned()));
        Ok(err(CellError::Name))
    }

    /// Invoke a callable, evaluating positional arguments in the caller frame.
    pub(super) fn invoke(
        &mut self,
        callee: Value,
        arguments: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        self.invoke_with(&callee, arguments.len(), |executor, index| {
            let argument = arguments.get(index).ok_or(EvaluationError::StalePlan)?;
            if argument.is_missing() {
                Ok(Value::Missing)
            } else {
                ExpressionRunner::new(executor.session).expr(
                    executor.target.sheet,
                    executor.target.address,
                    argument,
                )
            }
        })
    }

    /// Identify an executable closure without copying it or its captures.
    fn closure(callee: &Value, invalid: CellError) -> FunctionResult<&LambdaValue> {
        let callable = match callee {
            Value::Callable(callable) => callable,
            Value::Scalar(CellValue::Error(error))
            | Value::SelectedScalar(CellValue::Error(error), _) => return Err((*error).into()),
            _ => return Err(invalid.into()),
        };
        (callable.as_ref() as &dyn std::any::Any)
            .downcast_ref::<LambdaValue>()
            .ok_or_else(|| EvaluationError::StalePlan.into())
    }

    /// Build the callable's frame and unwind it identically for both argument sources.
    fn invoke_with(
        &mut self,
        callee: &Value,
        arity: usize,
        mut argument: impl FnMut(&mut Self, usize) -> EvalResult<Value>,
    ) -> EvalResult<Value> {
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::Expression, 1)?;
        let closure = match Self::closure(callee, CellError::Value) {
            Ok(closure) => closure,
            Err(FunctionError::Spreadsheet(error)) => return Ok(err(error)),
            Err(FunctionError::Operational(error)) => return Err(error),
        };

        if !(closure.minimum_arity..=closure.arity).contains(&arity) {
            return Ok(err(CellError::Value));
        }

        if self.session.state.runtime.vm.callable_depth >= MAX_CALLABLE_DEPTH {
            return Ok(err(CellError::Num));
        }

        let width = closure
            .captures
            .len()
            .checked_add(closure.arity)
            .ok_or(EvaluationError::StalePlan)?;
        if width > closure.program.template.local_slots as usize {
            return Err(EvaluationError::StalePlan);
        }
        self.session.state.runtime.budget.allocation(width)?;
        let mut locals = Vec::new();
        reserve(&mut locals, width, self.target)?;
        for captured in &closure.captures {
            let value = copy_value(&mut self.session.state.runtime.budget, captured)?;
            // SAFETY: The reservation includes all captures and arguments.
            locals.push(value);
        }
        for index in 0..closure.arity {
            let value = if index < arity {
                argument(self, index)?
            } else {
                Value::Missing
            };
            // SAFETY: The original reservation includes each positional argument.
            locals.push(value);
        }

        let runtime = &mut *self.session.state.runtime;
        let base = runtime.vm.locals.len();
        runtime
            .budget
            .check_allocation(base.checked_add(width).ok_or(EvaluationError::StalePlan)?)?;
        reserve(&mut runtime.vm.locals, width, self.target)?;
        let previous_depth = runtime.vm.callable_depth;
        let nested_depth = previous_depth
            .checked_add(1)
            .ok_or(EvaluationError::StalePlan)?;
        // SAFETY: The reservation admitted all captures and positional arguments.
        runtime.vm.locals.extend(locals);
        runtime.vm.callable_depth = nested_depth;
        let result = Vm::new(self.session).execute(
            self.target.sheet,
            self.target.address,
            &closure.program,
            closure.entry,
            Some(LocalFrame::new(base)),
        );
        let machine = &mut self.session.state.runtime.vm;
        machine.callable_depth = previous_depth;
        machine.locals.truncate(base);
        result
    }
}

/// Reserve callable storage and keep the native allocation failure attributable.
fn reserve<T>(values: &mut Vec<T>, additional: usize, target: EvaluationTarget) -> EvalResult<()> {
    values.try_reserve(additional).map_err(|source| {
        EvaluationError::reservation(
            CapabilityId::FunctionAdapter,
            Some(target),
            EvaluationReasonError::AllocatingAFunctionResultOrScratchBuffer,
            source,
        )
    })
}
