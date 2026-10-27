//! The value channel a consuming operation requests from an expression.

use crate::eval::values::Value;

/// The consuming operation selects reference expansion independently of worksheet publication.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::eval) enum ExpressionDemand {
    /// Ordinary argument and worksheet formulas retain reference implicit intersection.
    #[default]
    Scalar,
    /// Array consumers retain rectangular operator inputs.
    Array,
}

impl ExpressionDemand {
    /// Explicit arrays retain shape; references expand only for an array consumer or dynamic execution.
    pub(in crate::eval) fn retains_array(self, value: &Value) -> bool {
        matches!(value, Value::Array(_))
            || (self.expands_operator_references() && is_array_operand(value))
    }

    /// Whether operator reference operands keep their rectangular shape.
    pub(in crate::eval) fn expands_operator_references(self) -> bool {
        self == Self::Array
    }
}

/// Whether an operand needs array-mode selection rather than ordinary scalar projection.
fn is_array_operand(value: &Value) -> bool {
    match value {
        Value::Array(_) => true,
        // SAFETY: Short-circuit evaluation reaches element zero only when areas.len() == 1.
        Value::Refs(areas) | Value::SheetSpan(areas) => areas.len() != 1 || areas[0].size() != 1,
        // SAFETY: Short-circuit evaluation reaches element zero only when areas.len() == 1.
        Value::SignedRefs(areas) => areas.len() != 1 || areas[0].size() != 1,
        Value::Scalar(_) | Value::SelectedScalar(_, _) | Value::Missing => false,
    }
}
