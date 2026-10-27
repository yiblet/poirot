//! Common registration, deferred inputs, and composition for evaluator adapters.
//!
//! Depends on: compile (bound arguments), session (call context), vm (deferred
//! arguments, resolved calls and admission), config (builtin policy), functions
//! (identities), coordinates (cell targets), limits (budgets), values (value channels),
//! ports (dependency reads) and observe (consumed builtin invocations).

/// Evaluator services and caller coordinates supplied to a function adapter.
mod call_context;
/// A family registers names and executes its own handlers.
mod function_adapter;
/// A deferred expression memoized after successful evaluation.
mod function_input;
/// Borrowed lazy argument blocks from compiled bytecode.
mod function_parameters;
/// An adapter result may retain references or arrays until the caller consumes it.
mod function_value;
/// Unit tests for deferred adapter arguments.
#[cfg(test)]
mod tests;

pub use call_context::CallContext;
pub use function_adapter::FunctionAdapter;
pub use function_input::FunctionInput;
pub use function_parameters::FunctionParameters;
pub use function_value::FunctionValue;
