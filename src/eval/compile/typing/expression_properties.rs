use super::formula_effects::FormulaEffects;
use crate::model::{FormulaType, ShapeBounds};

/// Inferred possible values, maximum array dimensions, and effects for an expression.
/// Preparation uses this summary to bound possible spill geometry without executing
/// the expression or claiming that it will produce an array.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExpressionProperties {
    /// Possible runtime result forms and scalar element kinds.
    pub result: FormulaType,
    /// Independent upper bounds on successful materialized dimensions.
    pub shape: ShapeBounds,
    /// Conservative possible effects; actual traces govern retained-result validity.
    pub effects: FormulaEffects,
}
