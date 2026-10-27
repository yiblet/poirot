use super::arguments::Arguments;
use crate::eval::functions::registry::EngineeringFunction;

/// Bound engineering call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineeringExpr {
    /// Selected engineering handler.
    pub function: EngineeringFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
