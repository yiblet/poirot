use super::arguments::Arguments;
use crate::eval::functions::registry::MathFunction;

/// Bound mathematical call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct MathExpr {
    /// Selected math handler.
    pub function: MathFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
