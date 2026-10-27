use crate::eval::compile::bound;
use crate::eval::compile::compiler::PreparedFormula;
use crate::eval::coordinates::EvaluationTarget;

/// A requested cell, prepared formula, or bound expression evaluated at a target cell.
/// Several probes in one [`EvaluationRun`](super::EvaluationRun) share that run's memoized dependencies.
pub enum EvaluationProbe<'a> {
    /// Evaluate the workbook cell at the requested target.
    Cell(EvaluationTarget),
    /// Execute a supplied prepared formula at the requested target.
    Formula {
        /// Coordinates used for relative evaluation and attribution.
        target: EvaluationTarget,
        /// Prepared formula independent of the stored cell source.
        formula: &'a PreparedFormula,
    },
    /// Execute supplied bound syntax at the requested target.
    Bound {
        /// Coordinates used for relative evaluation and attribution.
        target: EvaluationTarget,
        /// Bound syntax to execute.
        formula: &'a bound::FormulaPlan,
    },
}
