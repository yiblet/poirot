//! Per-run evaluator state and the executors that run one cell lifecycle over it.
//!
//! `Session` is composed of purpose structs (sources, policies, run state, reuse
//! handles and observation ports). `RunState` reaches the persistent `Runtime` storage, owned
//! by the run or borrowed from a persistent workspace, which is itself composed of one
//! component per concern: VM stacks, budget and usage, the evaluation stack, the result
//! memo, call effects, spill preparation, lookup prefixes and observation state.
//! Behavior that needs the whole session runs through executor structs that borrow
//! it: `cells` reads and evaluates cells, `reads` records dependencies and reads
//! arguments, ranges and names, `projection` turns evaluated values into worksheet
//! results and coerced scalars, and `ObservationRecorder` emits execution observation
//! events.
//!
//! Depends on: compile (compiled plans and source preparation), config (execution
//! policies and random streams), functions (extension registry and trace), coordinates
//! (targets, coordinate tables and memo), limits (budgets), values (value channels and
//! reference geometry), host (deferred operands folded by sparse reads), reuse
//! (retained results, lookup prefixes and read recording), ports (observer, read and
//! cache ports), inputs (effective workbook), spill (spill candidates, preparation and
//! publication), vm (the VM stacks held in run storage, and formula and argument
//! execution: cells execute formulas and formulas read cells), and observe (observation
//! state and events).

/// Per-cell side effects of function calls.
mod call_effects;
/// Recursive cell reads and formula execution at a target.
mod cells;
/// The composed per-run evaluator state.
mod composition;
/// Cells and names under evaluation and the consuming target.
mod evaluation_stack;
/// The value channel a consumer requests from an expression.
mod expression_demand;
/// Executor that emits execution observation events and maintains the active frame.
mod observation_recorder;
/// Final worksheet results, legacy scalars and context-dependent coercion.
mod projection;
/// Dependency recording and argument, sparse range and name reads.
mod reads;
/// Memoized cell results and their retention accounting.
mod result_memo;
/// Shared handles that let a run reuse and record incremental evidence.
mod reuse_handles;
/// Execution policies selected for one run.
mod run_policies;
/// Optional ports that observe reads, cells, builtin calls and execution.
mod run_ports;
/// Workbook, compiled plan and compiler services a run reads from.
mod run_sources;
/// Persistent execution storage and per-call execution mode.
mod run_state;
/// Persistent execution storage retained across cells, composed of purpose structs.
mod runtime;
/// Owned or borrowed execution storage.
mod storage;

#[cfg(test)]
pub(in crate::eval) use cells::SOURCE_RESOLUTIONS;
pub(in crate::eval) use cells::{CellEvaluator, FormulaExecution, ResolvedSource};
pub(in crate::eval) use composition::Session;
pub(in crate::eval) use expression_demand::ExpressionDemand;
pub(in crate::eval) use observation_recorder::ObservationRecorder;
pub(in crate::eval) use projection::{Coercer, ResultProjection};
pub(in crate::eval) use reads::{
    ArgumentReader, ItemFolder, NameResolver, ReadRecorder, SparseMember, SparseReader,
};
pub(in crate::eval) use run_policies::RunPolicies;
pub(in crate::eval) use runtime::Runtime;
pub(in crate::eval) use storage::Storage;
