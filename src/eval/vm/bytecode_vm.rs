//! Stack execution of owned instructions; calls borrow lazy blocks through family hosts.
//! The unit-test tree oracle remains to check value, read, observation and budget parity.

use super::arguments_ref::ArgumentsRef;
use super::operators::Operators;
use super::reference_atoms::ReferenceAtoms;
use super::reference_ref::ReferenceRef;
use crate::eval::compile::bytecode::{
    CallTarget, Compiled, Label, Opcode, OperandCursor, ReferenceKind,
};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::functions::registry::{FunctionId, LogicalFunction};
use crate::eval::limits::{CapabilityId, EvalResult, EvaluationError, WorkKind};
use crate::eval::observe::{ConsumedBuiltinInvocation, Operation, OperationKind, observed_value};
use crate::eval::ports::DependencyRead;
use crate::eval::session::{
    NameResolver, ObservationRecorder, ReadRecorder, ResultProjection, Session,
};
use crate::eval::values::{
    ArrayView, VALUE_ERROR, Value, convert_legacy_missing_blank, err, numeric_outcome,
};
use crate::eval::{adapters, functions, host};
use crate::model::CellAddress;
use crate::model::CellError::{Name, Ref};
use crate::model::CellValue::{Boolean, Error, Text};
use poirot_parsing::formula::{BinaryOp, UnaryOp};

/// Runs compiled bytecode for one formula block on the session's shared stacks.
pub(in crate::eval) struct Vm<'s, 'a> {
    session: &'s mut Session<'a>,
}

/// Active operation and prior observation context restored when a bytecode expression returns.
type ObservationFrame = (Operation, Option<(Operation, u64)>);

impl<'s, 'a> Vm<'s, 'a> {
    /// Borrow the session whose stacks, budgets and observers execution uses.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Check extension identity and execute a complete compiled formula block.
    pub(in crate::eval) fn run(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
    ) -> EvalResult<Value> {
        if program.extension_identity.as_ref().is_some_and(|identity| {
            self.session
                .sources
                .extensions
                .is_none_or(|r| &r.identity() != identity)
        }) {
            return Err(EvaluationError::UnsupportedCapability {
                capability: CapabilityId::UserDefinedFunction,
                target: Some(EvaluationTarget { sheet, address: at }),
                reason: crate::eval::limits::EvaluationReasonError::ExtensionRegistryIdentityChangedRebindTheFormulaPlan,
            });
        }
        self.execute(sheet, at, program, 0)
    }

    /// Execute a bytecode block with shared budgets and lazy family adapters.
    pub(in crate::eval) fn execute(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
        start: usize,
    ) -> EvalResult<Value> {
        let base = self.session.state.runtime.vm.values.len();
        let observed = self.session.ports.execution_observer.is_some();
        let mut frames: Vec<ObservationFrame> = Vec::new();
        let mut entered = 0usize;
        let result = (|| {
            let mut ip = start;
            loop {
                let instruction = *program
                    .template
                    .code
                    .get(ip)
                    .ok_or(EvaluationError::StalePlan)?;
                match instruction.opcode {
                    Opcode::Enter => {
                        self.session
                            .state
                            .runtime
                            .budget
                            .charge(WorkKind::Expression, 1)?;
                        self.session.state.runtime.budget.enter_depth()?;
                        if observed {
                            let handler = program
                                .template
                                .code
                                .get(ip + 1)
                                .filter(|next| next.opcode.is_call())
                                .and_then(|next| self.handler(program, next.a as usize));
                            match ObservationRecorder::new(self.session).observation_begin(
                                EvaluationTarget { sheet, address: at },
                                OperationKind::Expression { handler },
                            ) {
                                Ok(frame) => frames.push(frame),
                                Err(error) => {
                                    self.session.state.runtime.budget.leave_depth();
                                    return Err(error);
                                }
                            }
                        }
                        entered += 1;
                    }
                    Opcode::Leave => {
                        entered = entered.checked_sub(1).ok_or(EvaluationError::StalePlan)?;
                        let notification = if let Some((operation, previous)) = frames.pop() {
                            let value = self
                                .session
                                .state
                                .runtime
                                .vm
                                .values
                                .pop()
                                .ok_or(EvaluationError::StalePlan)?;
                            let notification = ObservationRecorder::new(self.session)
                                .observation_end(operation, previous, Ok(observed_value(&value)));
                            self.session.state.runtime.vm.values.push(value);
                            notification
                        } else {
                            Ok(())
                        };
                        self.session.state.runtime.budget.leave_depth();
                        notification?;
                        if entered == 0 {
                            if self.session.state.runtime.vm.values.len() != base.saturating_add(1)
                            {
                                return Err(EvaluationError::StalePlan);
                            }
                            return self
                                .session
                                .state
                                .runtime
                                .vm
                                .values
                                .pop()
                                .ok_or(EvaluationError::StalePlan);
                        }
                    }
                    Opcode::Number => {
                        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                        let value = numeric_outcome::convert_numeric_literal(f64::from_bits(
                            u64::from(instruction.a) | u64::from(instruction.b) << 32,
                        ))?
                        .into();
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::Text => {
                        let text = program.text(instruction.a, instruction.b)?;
                        self.session.state.runtime.budget.text(text.len())?;
                        self.session
                            .state
                            .runtime
                            .vm
                            .values
                            .push(Text(text.to_owned()).into());
                    }
                    Opcode::Boolean => self
                        .session
                        .state
                        .runtime
                        .vm
                        .values
                        .push(Boolean(instruction.a != 0).into()),
                    Opcode::Error => self.session.state.runtime.vm.values.push(
                        Error(
                            program
                                .text(instruction.a, instruction.b)?
                                .parse()
                                .unwrap_or(VALUE_ERROR),
                        )
                        .into(),
                    ),
                    // TODO(coercion-non-argument): [MEDIUM] VM omission-to-blank projection can diverge from deferred calls that preserve missing syntax.
                    Opcode::Missing => self
                        .session
                        .state
                        .runtime
                        .vm
                        .values
                        .push(convert_legacy_missing_blank().into()),
                    Opcode::Reference => {
                        let value = self.reference(sheet, at, program, instruction.a as usize)?;
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::Name => {
                        let value = NameResolver::new(self.session).defined_name(
                            sheet,
                            sheet,
                            at,
                            program.text(instruction.a, instruction.b)?,
                        )?;
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::QualifiedName => {
                        let mut bytes = OperandCursor(
                            program
                                .template
                                .operands
                                .get(
                                    instruction.a as usize
                                        ..instruction
                                            .a
                                            .checked_add(instruction.b)
                                            .ok_or(EvaluationError::StalePlan)?
                                            as usize,
                                )
                                .ok_or(EvaluationError::StalePlan)?,
                        );
                        let name_sheet = bytes.string()?;
                        let name =
                            std::str::from_utf8(bytes.0).map_err(|_| EvaluationError::StalePlan)?;
                        let value = match NameResolver::new(self.session)
                            .sheet_named(sheet, Some(name_sheet))?
                        {
                            Some(scope) => NameResolver::new(self.session)
                                .defined_name(sheet, scope, at, name)?,
                            None => err(Ref),
                        };
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::Unary => {
                        let value = self
                            .session
                            .state
                            .runtime
                            .vm
                            .values
                            .pop()
                            .ok_or(EvaluationError::StalePlan)?;
                        let value = Operators::new(self.session).unary_value(
                            sheet,
                            at,
                            decode_unary(instruction.a)?,
                            value,
                        )?;
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::Binary
                    | Opcode::Add
                    | Opcode::Subtract
                    | Opcode::Multiply
                    | Opcode::Divide
                    | Opcode::Power
                    | Opcode::Concat
                    | Opcode::Equal
                    | Opcode::NotEqual
                    | Opcode::Less
                    | Opcode::LessEqual
                    | Opcode::Greater
                    | Opcode::GreaterEqual => {
                        let right = self
                            .session
                            .state
                            .runtime
                            .vm
                            .values
                            .pop()
                            .ok_or(EvaluationError::StalePlan)?;
                        let left = self
                            .session
                            .state
                            .runtime
                            .vm
                            .values
                            .pop()
                            .ok_or(EvaluationError::StalePlan)?;
                        let value = Operators::new(self.session).binary_values(
                            sheet,
                            at,
                            instruction
                                .opcode
                                .binary_operation()
                                .map_or_else(|| decode_binary(instruction.a), Ok)?,
                            left,
                            right,
                        )?;
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::ArrayAllocate => self
                        .session
                        .state
                        .runtime
                        .budget
                        .allocation(instruction.a as usize)?,
                    Opcode::Scalarize => {
                        let value = self
                            .session
                            .state
                            .runtime
                            .vm
                            .values
                            .pop()
                            .ok_or(EvaluationError::StalePlan)?;
                        let value = ResultProjection::new(self.session)
                            .select_legacy_scalar(at, value)?
                            .into();
                        self.session.state.runtime.vm.values.push(value);
                    }
                    Opcode::Array => {
                        let count = (instruction.a as usize)
                            .checked_mul(instruction.b as usize)
                            .ok_or(EvaluationError::StalePlan)?;
                        let offset = self
                            .session
                            .state
                            .runtime
                            .vm
                            .values
                            .len()
                            .checked_sub(count)
                            .ok_or(EvaluationError::StalePlan)?;
                        // SAFETY: offset was obtained by checked subtraction from self.runtime.vm.values.len(); the vector has not changed since that check.
                        let mut elements = self.session.state.runtime.vm.values.drain(offset..);
                        let mut rows = Vec::new();
                        for _ in 0..instruction.a {
                            let mut row = Vec::new();
                            for _ in 0..instruction.b {
                                let Some(Value::Scalar(value)) = elements.next() else {
                                    return Err(EvaluationError::StalePlan);
                                };
                                row.push(value);
                            }
                            rows.push(row);
                        }
                        drop(elements);
                        self.session.state.runtime.vm.values.push(Value::Array(
                            ArrayView::anchored(rows, Some(CellAddress { row: 0, col: 0 })),
                        ));
                    }
                    Opcode::Invoke | Opcode::Conditional => {
                        if instruction.opcode == Opcode::Conditional
                            && self.handler(program, instruction.a as usize)
                                != Some(FunctionId::Logical(LogicalFunction::If))
                        {
                            return Err(EvaluationError::StalePlan);
                        }
                        let args = ArgumentsRef::Bytecode {
                            program,
                            first: ip + 2,
                            count: instruction.b as usize,
                        };
                        let value = self.call(sheet, at, program, instruction.a as usize, args)?;
                        self.session.state.runtime.vm.values.push(value);
                        ip = program
                            .template
                            .resolve_label(Label(instruction.c))
                            .ok_or(EvaluationError::StalePlan)?
                            .index();
                        continue;
                    }
                    Opcode::Jump => {
                        ip = program
                            .template
                            .resolve_label(Label(instruction.c))
                            .ok_or(EvaluationError::StalePlan)?
                            .index();
                        continue;
                    }
                    Opcode::YieldArgument | Opcode::Argument => {
                        return Err(EvaluationError::StalePlan);
                    }
                }
                ip += 1;
            }
        })();
        if let Err(error) = &result {
            while entered != 0 {
                if let Some((operation, previous)) = frames.pop() {
                    // Existing operational failures dominate observer failures.
                    let _ = ObservationRecorder::new(self.session).observation_end(
                        operation,
                        previous,
                        Err(error),
                    );
                }
                self.session.state.runtime.budget.leave_depth();
                entered -= 1;
            }
        }
        // Nested cell and deferred-argument calls share this stack. Only the
        // current frame's temporaries are discarded; caller values remain in place.
        self.session.state.runtime.vm.values.truncate(base);
        result
    }

    /// Builtin selected by a compiled call slot.
    fn handler(&self, program: &Compiled, index: usize) -> Option<FunctionId> {
        match &program.calls.get(index)?.target {
            CallTarget::Builtin(handler) => Some(*handler),
            CallTarget::Unresolved => {
                functions::registry::resolve_handler(&poirot_parsing::formula::FunctionName::from(
                    program.resolved_spelling(index).ok()?,
                ))
            }
            _ => None,
        }
    }

    /// Admit and dispatch one bytecode call with its deferred arguments.
    fn call(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
        index: usize,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        let call = program.calls.get(index).ok_or(EvaluationError::StalePlan)?;
        let handler = self.handler(program, index);
        let parsed_spelling = program.source_spelling(index)?;
        let resolved_spelling = program.resolved_spelling(index)?;
        self.session
            .ports
            .observe_resolved_call(handler.map(|handler| ConsumedBuiltinInvocation {
                target: EvaluationTarget { sheet, address: at },
                handler,
                parsed_spelling,
                resolved_spelling,
                argument_count: args.len(),
            }))?;
        if let Some(handler) = handler {
            return adapters::execute(
                self.session,
                EvaluationTarget { sheet, address: at },
                handler,
                args,
            );
        }
        match &call.target {
            CallTarget::Extension(binding) => {
                host::execute_extension(self.session, sheet, at, binding, args)
            }
            _ => {
                ReadRecorder::new(self.session)
                    .read_lazy(|| DependencyRead::OpaqueProvider(parsed_spelling.to_owned()));
                Ok(err(Name))
            }
        }
    }

    /// Decode one reference slot into a value channel.
    fn reference(
        &mut self,
        sheet: usize,
        at: CellAddress,
        program: &Compiled,
        slot: usize,
    ) -> EvalResult<Value> {
        let binding = *program
            .references
            .slots
            .get(slot)
            .ok_or(EvaluationError::StalePlan)?;
        let mut reader = OperandCursor(
            program
                .references
                .data
                .get(
                    binding.offset as usize
                        ..binding
                            .offset
                            .checked_add(binding.length)
                            .ok_or(EvaluationError::StalePlan)? as usize,
                )
                .ok_or(EvaluationError::StalePlan)?,
        );
        match binding.kind {
            ReferenceKind::Cell => {
                let row = reader.word()?;
                let column = reader.word()?;
                reader.word()?;
                reader.word()?;
                let named = reader.optional()?;
                ReferenceAtoms::new(self.session).resolve(
                    sheet,
                    at,
                    ReferenceRef::Cell {
                        row,
                        column,
                        sheet: named,
                    },
                )
            }
            ReferenceKind::Column | ReferenceKind::Row => {
                let index = reader.word()?;
                reader.word()?;
                let named = reader.optional()?;
                ReferenceAtoms::new(self.session).resolve(
                    sheet,
                    at,
                    match binding.kind {
                        ReferenceKind::Column => ReferenceRef::Column {
                            column: index,
                            sheet: named,
                        },
                        _ => ReferenceRef::Row {
                            row: index,
                            sheet: named,
                        },
                    },
                )
            }
            ReferenceKind::SheetSpan => {
                let first_sheet = reader.string()?;
                let last_sheet = reader.string()?;
                let start = reader.cell()?;
                let end = if reader.word()? != 0 {
                    Some(reader.cell()?)
                } else {
                    None
                };
                ReferenceAtoms::new(self.session).resolve(
                    sheet,
                    at,
                    ReferenceRef::SheetSpan {
                        first_sheet,
                        last_sheet,
                        start: &start,
                        end: end.as_ref(),
                    },
                )
            }
        }
    }
}

/// Decode a packed unary operator code.
pub(super) fn decode_unary(code: u32) -> EvalResult<UnaryOp> {
    Ok(match code {
        0 => UnaryOp::Plus,
        1 => UnaryOp::Minus,
        2 => UnaryOp::Percent,
        3 => UnaryOp::ImplicitIntersection,
        4 => UnaryOp::SpillRange,
        _ => return Err(EvaluationError::StalePlan),
    })
}

/// Decode a packed binary operator code.
pub(super) fn decode_binary(code: u32) -> EvalResult<BinaryOp> {
    Ok(match code {
        0 => BinaryOp::Add,
        1 => BinaryOp::Subtract,
        2 => BinaryOp::Multiply,
        3 => BinaryOp::Divide,
        4 => BinaryOp::Power,
        5 => BinaryOp::Concat,
        6 => BinaryOp::Equal,
        7 => BinaryOp::NotEqual,
        8 => BinaryOp::Less,
        9 => BinaryOp::LessEqual,
        10 => BinaryOp::Greater,
        11 => BinaryOp::GreaterEqual,
        12 => BinaryOp::Range,
        13 => BinaryOp::Union,
        14 => BinaryOp::Intersection,
        _ => return Err(EvaluationError::StalePlan),
    })
}
