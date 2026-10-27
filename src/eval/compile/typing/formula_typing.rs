use super::expression_properties::ExpressionProperties;
use super::typed_expression::TypedExpression;
use crate::eval::compile::bytecode::{Opcode, Template};
use crate::model::ShapeBounds;

/// Inference results for every expression in a prepared executable.
/// Root properties bound possible spill geometry; intermediate entries preserve
/// the contracts associated with bytecode expression boundaries.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FormulaTyping {
    pub(super) expressions: Vec<TypedExpression>,
}

impl FormulaTyping {
    /// Root properties of this prepared formula/cell, when it contains an expression.
    pub fn root(&self) -> Option<ExpressionProperties> {
        self.expressions.first().map(|e| e.properties)
    }

    /// Properties of all executable expression boundaries, in instruction order.
    pub fn expressions(&self) -> &[TypedExpression] {
        &self.expressions
    }

    /// Check that serialized properties cover exactly the executable expression boundaries.
    pub(in crate::eval) fn matches(&self, template: &Template) -> bool {
        let mut typed = self.expressions.iter();
        for (i, instruction) in template.code.iter().enumerate() {
            if instruction.opcode == Opcode::Enter
                && typed
                    .next()
                    .is_none_or(|t| usize::try_from(t.instruction).ok() != Some(i))
            {
                return false;
            }
        }
        typed.next().is_none()
            && !self.expressions.is_empty()
            && self.expressions.iter().all(|entry| {
                entry.properties.result.may_spill() || entry.properties.shape == ShapeBounds::SCALAR
            })
    }
}
