use crate::eval::config::EvaluationInputs;
use crate::eval::functions::extensions::ExtensionRegistry;
use crate::eval::limits::EvaluationBudgets;

/// Registry, captured context, and budgets for one extension-enabled evaluation.
/// Use with [`run_formula_with_extensions`](crate::eval::run_formula_with_extensions) or
/// [`run_cell_with_extensions`](crate::eval::run_cell_with_extensions); the overlay
/// builder accepts the runtime extension registry.
pub struct ExtensionExecution<'a> {
    /// Installed providers and exact binding identities.
    pub registry: &'a ExtensionRegistry,
    /// Captured clock, seed, and localization inputs.
    pub context: EvaluationInputs,
    /// Execution work and allocation limits.
    pub budgets: EvaluationBudgets,
}
