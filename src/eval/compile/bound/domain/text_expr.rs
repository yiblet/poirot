use super::arguments::Arguments;
use crate::eval::functions::registry::TextFunction;

/// Bound text call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct TextExpr {
    /// Selected text handler.
    pub function: TextFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
