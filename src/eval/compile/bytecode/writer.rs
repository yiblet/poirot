use super::aggregation;
use super::bytes::Bytes;
use super::call::Call;
use super::call_target::CallTarget;
use super::compile_error::CompileError;
use super::compiled::Compiled;
use super::high::High;
use super::instruction::Instruction;
use super::instruction_index::InstructionIndex;
use super::label::Label;
use super::lowering::{DEPTH, NONE, binary, reference, unary};
use super::opcode::Opcode;
use super::reference_binding::ReferenceBinding;
use super::sink::Sink;
use poirot_parsing::formula::Expr;

/// Lowers streamed operations into final buffers and resolves label fixups.
pub(super) struct Writer<'registry> {
    pub(super) error: Option<CompileError>,
    pub(super) registry: Option<&'registry crate::eval::functions::extensions::ExtensionRegistry>,
    pub(super) output: Compiled,
    pub(super) pending_labels: [Option<Label>; DEPTH * 2],
    /// Zero-based reducer argument at each nesting depth: GROUPBY uses 2, PIVOTBY 3.
    pub(super) aggregation_roles: [Option<usize>; DEPTH],
}

impl Writer<'_> {
    pub(super) fn text(&mut self, s: &str) -> (u32, u32) {
        let start = self.output.template.operands.len() as u32;
        self.output
            .template
            .operands
            .extend_from_slice(s.as_bytes());
        (start, s.len() as u32)
    }

    pub(super) fn atom(&mut self, e: &Expr) -> Instruction {
        let offset = self.output.references.data.len() as u32;
        if let Some(kind) = reference(e, &mut self.output.references.data) {
            let slot = self.output.template.reference_slots;
            self.output.template.reference_slots += 1;
            self.output.references.slots.push(ReferenceBinding {
                kind,
                offset,
                length: self.output.references.data.len() as u32 - offset,
            });
            return Instruction::new(Opcode::Reference, slot, 0);
        }
        match e {
            Expr::Number(n) => {
                let bits = n.to_bits();
                Instruction::new(Opcode::Number, bits as u32, (bits >> 32) as u32)
            }
            Expr::Bool(b) => Instruction::new(Opcode::Boolean, (*b).into(), 0),
            Expr::Missing => Instruction::new(Opcode::Missing, 0, 0),
            Expr::String(s) | Expr::Error(s) | Expr::Name(s) => {
                let opcode = match e {
                    Expr::String(_) => Opcode::Text,
                    Expr::Error(_) => Opcode::Error,
                    _ => Opcode::Name,
                };
                let (a, b) = self.text(s);
                Instruction::new(opcode, a, b)
            }
            Expr::QualifiedName { sheet, name } => {
                let a = self.output.template.operands.len() as u32;
                self.output.template.operands.string(sheet);
                self.output
                    .template
                    .operands
                    .extend_from_slice(name.as_bytes());
                Instruction::new(
                    Opcode::QualifiedName,
                    a,
                    (4 + sheet.len() + name.len()) as u32,
                )
            }
            _ => {
                self.error = Some(CompileError::InvalidReference);
                Instruction::new(Opcode::Missing, 0, 0)
            }
        }
    }
}

impl<'ast> Sink<'ast> for Writer<'_> {
    fn emit(&mut self, op: High<'ast>) {
        if self.error.is_some() {
            return;
        }
        if let High::Label(label) = op {
            let destination = self.output.template.code.len() as u32;
            let Some(pending) = self.pending_labels.get_mut(label) else {
                self.error = Some(CompileError::ResourceLimit);
                return;
            };
            // A zero-argument call still has a referenced continuation label.
            if let Some(label) = pending.take() {
                let Some(slot) = self.output.template.labels.get_mut(label.0 as usize) else {
                    self.error = Some(CompileError::InvalidReference);
                    return;
                };
                *slot = InstructionIndex(destination);
            } else {
                self.error = Some(CompileError::InvalidReference);
            }
            return;
        }
        let mut target = None;
        let mut instruction = match op {
            High::Enter(d, _) => Instruction::new(Opcode::Enter, d as u32, 0),
            High::Leave => Instruction::new(Opcode::Leave, 0, 0),
            High::Atom(e) => self.atom(e),
            High::Truncated => {
                let (a, b) = self.text("#NUM!");
                Instruction::new(Opcode::Error, a, b)
            }
            High::Unary(op) => Instruction::new(Opcode::Unary, unary(op), 0),
            High::Binary(op) => Instruction::new(Opcode::for_binary(op), binary(op), 0),
            High::ArrayAllocate(rows, columns) => {
                let mut instruction =
                    Instruction::new(Opcode::ArrayAllocate, (rows * columns) as u32, rows as u32);
                instruction.c = columns as u32;
                instruction
            }
            High::Scalarize => Instruction::new(Opcode::Scalarize, 0, 0),
            High::Array(r, c) => Instruction::new(Opcode::Array, r as u32, c as u32),
            High::Invoke(name, count, label) => {
                let a = self.output.template.operands.len() as u32;
                self.output.template.operands.string(name);
                let index = self.output.calls.len() as u32;
                let lookup = match self.registry.map(|r| r.external_name_lookup(name)) {
                    Some(crate::eval::functions::external_namespace::ExternalNameLookup::Stored(stored)) => {
                        Some(stored)
                    }
                    Some(crate::eval::functions::external_namespace::ExternalNameLookup::UnsupportedName) => {
                        None
                    }
                    _ => Some(name),
                };
                let call_target = match lookup {
                    Some(lookup) => match crate::eval::functions::registry::resolve_function(
                        &poirot_parsing::formula::FunctionName::from(lookup),
                    ) {
                        Some(d) => CallTarget::Builtin(d.handler),
                        None => self
                            .registry
                            .and_then(|r| r.resolve(lookup))
                            .map_or(CallTarget::Unresolved, CallTarget::Extension),
                    },
                    None => CallTarget::Unresolved,
                };
                if let Some(role) = self.aggregation_roles.get_mut(label / 2) {
                    *role = if matches!(call_target, CallTarget::Extension(_)) {
                        None
                    } else {
                        match lookup.and_then(|name| {
                            crate::eval::functions::registry::resolve_handler(&name.into())
                        }) {
                            Some(crate::eval::functions::registry::FunctionId::Matrix(
                                crate::eval::functions::registry::MatrixFunction::Groupby,
                            )) => Some(2),
                            Some(crate::eval::functions::registry::FunctionId::Matrix(
                                crate::eval::functions::registry::MatrixFunction::Pivotby,
                            )) => Some(3),
                            _ => None,
                        }
                    };
                }
                let opcode = if call_target
                    == CallTarget::Builtin(crate::eval::functions::registry::FunctionId::Logical(
                        crate::eval::functions::registry::LogicalFunction::If,
                    )) {
                    Opcode::Conditional
                } else {
                    Opcode::Invoke
                };
                self.output.calls.push(Call {
                    source_offset: a,
                    resolved_name: lookup.filter(|s| *s != name).map(str::to_owned),
                    target: call_target,
                });
                target = Some(label);
                Instruction::new(opcode, index, count as u32)
            }
            High::Argument(index, label, depth, expression) => {
                if self.aggregation_roles.get(depth) == Some(&Some(index)) {
                    match aggregation::lower_with_registry(expression, self.registry) {
                        Ok(Some(plan)) => {
                            let start = self.output.template.code.len().saturating_add(1);
                            if self.output.template.aggregations.try_reserve(1).is_err() {
                                self.error = Some(CompileError::ResourceLimit);
                                return;
                            }
                            // SAFETY: The successful reservation admits one entry and nothing has been inserted since.
                            self.output
                                .template
                                .aggregations
                                .push(aggregation::Entry { start, plan });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.error = Some(error);
                            return;
                        }
                    }
                }
                target = Some(label);
                Instruction::new(Opcode::Argument, index as u32, 0)
            }
            High::YieldArgument => Instruction::new(Opcode::YieldArgument, 0, 0),
            High::Goto(label) => {
                target = Some(label);
                Instruction::new(Opcode::Jump, 0, 0)
            }
            // SAFETY: The High::Label branch above returns before this match.
            High::Label(_) => unreachable!(),
        };
        if let Some(label) = target {
            let Some(pending) = self.pending_labels.get_mut(label) else {
                self.error = Some(CompileError::ResourceLimit);
                return;
            };
            let label = match *pending {
                Some(label) => label,
                None => {
                    let label = Label(self.output.template.labels.len() as u32);
                    if self.output.template.labels.len() == self.output.template.labels.capacity() {
                        self.error = Some(CompileError::ResourceLimit);
                        return;
                    }
                    // SAFETY: The capacity check above proves one spare slot, and no vector
                    // mutation occurs before this push.
                    self.output.template.labels.push(InstructionIndex(NONE));
                    *pending = Some(label);
                    label
                }
            };
            instruction.c = label.0;
        }
        self.output.template.code.push(instruction);
    }
}
