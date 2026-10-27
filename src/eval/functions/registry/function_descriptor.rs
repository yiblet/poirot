use super::arity::Arity;
use super::dependency_policy::DependencyPolicy;
use super::evaluation_strategy::EvaluationStrategy;
use super::function_id::FunctionId;
use super::result_rule::ResultRule;

/// Advertised builtin name, typed handler, arity, evaluation strategy, and dependency flags.
/// This describes registration; it does not promise every input or execution mode is supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionDescriptor {
    /// Advertised formula spelling, including aliases.
    pub name: &'static str,
    /// Typed execution handler selected for this spelling.
    pub handler: FunctionId,
    /// Descriptive argument count contract.
    pub arity: Arity,
    /// Eager, lazy, or reference-aware argument evaluation strategy.
    pub strategy: EvaluationStrategy,
    /// Declared context, volatility, and reference dependencies.
    pub dependencies: DependencyPolicy,
    /// Possible result form and element-kind transformation.
    pub result: ResultRule,
}
