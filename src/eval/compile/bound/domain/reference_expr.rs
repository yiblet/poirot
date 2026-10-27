use super::arguments::Arguments;
use crate::eval::functions::registry::ReferenceFunction;

/// Bound reference call, preserving deferred reference operands.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceExpr {
    /// Selected reference handler.
    pub function: ReferenceFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
