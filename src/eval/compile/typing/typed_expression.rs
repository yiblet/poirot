use super::expression_contract::ExpressionContract;
use super::expression_properties::ExpressionProperties;

/// Connects an expression's inferred properties and evaluation contract to its bytecode entry.
/// The instruction offset keeps prepared analysis aligned with the expression the VM executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TypedExpression {
    /// Instruction offset of this expression's Enter opcode.
    pub instruction: u32,
    /// Possible results and characteristics at this boundary.
    pub properties: ExpressionProperties,
    /// Transfer and argument-demand contract for the expression operator.
    pub contract: ExpressionContract,
}
