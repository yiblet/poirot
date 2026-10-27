//! Bytecode execution: the VM and its reusable stacks, deferred expression dispatch,
//! operators, reference construction, call admission, rectangular selection and
//! broadcasting.
//! Behavior that needs the whole session runs through executor structs that borrow it.
//!
//! Depends on: compile (bytecode programs and bound trees), config (builtin policy),
//! functions (registry identities), coordinates (cell targets), limits (budgets and
//! failures), values (value channels and operator semantics), session (the borrowed
//! `Session`, its run storage and expression demand, and its cell, read, projection and
//! observation executors), host and adapters (call dispatch, extension invocation and
//! the deferred operands kept on the VM stacks), spill (spill references and
//! preparation), ports (dependency reads) and observe (execution events and consumed
//! calls).

/// Ordering of builtin admission relative to policy dependency reads.
mod admission_order;
/// Source-order iteration over deferred argument handles.
mod arguments_iter;
/// Deferred argument handles for one call.
mod arguments_ref;
/// Member-wise unary and binary array operators.
mod array_operators;
/// Selected-branch evaluation and broadcast expansion.
mod branch_selector;
/// The bytecode virtual machine executor.
mod bytecode_vm;
/// Observation and policy admission of resolved builtin calls.
mod call_admission;
/// Borrowed expression blocks.
mod executable_ref;
/// Deferred expression dispatch to the VM (or the test-only tree oracle).
mod expression_runner;
/// Reusable value and argument stacks.
mod machine;
/// Scalar and array-aware formula operators.
mod operators;
/// Rectangular array construction and broadcast member reads.
mod rectangular_traversal;
/// Reference construction from decoded geometry.
mod reference_atoms;
/// Reference union, range and intersection operators.
mod reference_operators;
/// Borrowed decoded reference geometry.
mod reference_ref;
/// Resolved builtin metadata carried into call admission.
mod resolved_call;
/// Rectangular branch selection states and broadcasting.
mod selection;
/// Rectangular operand dimensions.
mod shape;
/// Unit tests for the VM, call admission, branch selection and rectangular traversal.
#[cfg(test)]
mod tests;

pub(in crate::eval) use admission_order::AdmissionOrder;
pub(in crate::eval) use arguments_ref::ArgumentsRef;
pub(in crate::eval) use branch_selector::BranchSelector;
pub(in crate::eval) use bytecode_vm::Vm;
pub(in crate::eval) use call_admission::CallAdmission;
pub(in crate::eval) use executable_ref::ExecutableRef;
pub(in crate::eval) use expression_runner::ExpressionRunner;
pub(in crate::eval) use machine::Machine;
pub(in crate::eval) use rectangular_traversal::RectangularTraversal;
#[cfg(test)]
pub(in crate::eval) use reference_operators::ReferenceOperators;
pub(in crate::eval) use resolved_call::ResolvedCall;
pub(in crate::eval) use selection::{Selection, broadcast};
pub(in crate::eval) use shape::Shape;
