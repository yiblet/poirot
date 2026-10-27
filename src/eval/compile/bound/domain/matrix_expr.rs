use super::arguments::Arguments;
use crate::eval::functions::registry::MatrixFunction;

/// Bound matrix call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct MatrixExpr {
    /// Selected matrix handler.
    pub function: MatrixFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
