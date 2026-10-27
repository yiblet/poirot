//! Owned stack compiler and runtime metadata for lazy bytecode execution.
//!
//! Preparation validates and counts the source, then emits instructions and type facts
//! in one shared traversal. Each compiled body owns an instruction vector and a dense
//! label vector. Branches and deferred arguments occupy that same instruction buffer;
//! they do not own boxed child programs. Labels resolve to instruction positions.
//!
//! Operators consume values from the shared VM stack. They retain generic semantics:
//! `Add` handles both scalar operands and singleton-axis array broadcasting at runtime.
//! Operand types and shapes can change without rewriting or recompiling the instructions.
//! Type inference remains available for preparation and spill bounds; no runtime
//! specialization cache is required by this instruction format.
//!
//! Every call, including unknown and extension calls, owns deferred argument blocks.
//! Selection preserves function-specific arity, laziness, omission and error ordering.
//! Calls dispatch through function-family executors, which request only the argument
//! blocks they need. `Conditional` identifies `IF` calls and uses the same execution
//! path as `Invoke`; neither instruction eagerly executes every argument block.
//! References stay values until an operator or family adapter requests a read.
//!
//! Only final buffers and the shared executable owner allocate during compilation;
//! the streamed preparation events and temporary label bindings allocate nothing.
//! Source retention and template interning belong to the caller.
//!
//! Depends on: functions (registry identities and external names) and limits
//! (preparation budgets); siblings bound and typing.

pub mod aggregation;
/// Binary operand writer used to encode the executable's constant/reference storage.
mod bytes;
/// Resolved function identity and spelling metadata for one bytecode call.
mod call;
/// Handler identity selected during compilation, without retaining a provider object.
mod call_target;
/// Structural or resource failure while producing an executable program.
mod compile_error;
/// An executable shared by `Arc` plus reference coordinates for this formula.
mod compiled;
/// A template with resolved builtin/extension call identities.
mod executable;
/// Temporary high-level instruction stream lowered into fixed-width executable bytecode.
mod high;
/// Fixed-width stack instruction.
mod instruction;
/// Position in a compiled body's instruction vector.
mod instruction_index;
/// Index into a compiled body's label table, distinct from an instruction offset.
mod label;
/// Validation and lowering of bound expressions into bytecode.
mod lowering;
/// Stack operations and lazy argument descriptors understood by the VM.
mod opcode;
/// Checked decoder for operand bytes written by the bytecode writer.
mod operand_cursor;
/// Coordinates independent inference and bytecode machines over the same expression events.
mod preparation;
/// Locates one formula reference's encoded coordinates in [`ReferenceBindings::data`].
mod reference_binding;
/// Occurrence-ordered reference slots and their encoded per-formula coordinates and qualifiers.
mod reference_bindings;
/// Encoding discriminator for a formula's concrete reference binding.
mod reference_kind;
/// Bytecode lowering destination shared by sizing and emission passes.
mod sink;
/// Output buffer capacities counted before compilation allocates storage.
mod sizes;
/// Shared instructions and literal operand bytes.
mod template;
#[cfg(test)]
mod tests;
/// Lowers streamed operations into final buffers and resolves label fixups.
mod writer;

pub use call::Call;
pub use call_target::CallTarget;
pub use compile_error::CompileError;
pub use compiled::Compiled;
pub use executable::Executable;
pub use instruction::Instruction;
pub use instruction_index::InstructionIndex;
pub use label::Label;
pub(in crate::eval) use lowering::DEPTH;
pub use lowering::{compile, compile_aggregation_argument, compile_with_registry, lower_parsed};
pub use opcode::Opcode;
pub(in crate::eval) use operand_cursor::OperandCursor;
pub use reference_binding::ReferenceBinding;
pub use reference_bindings::ReferenceBindings;
pub use reference_kind::ReferenceKind;
pub use template::Template;
