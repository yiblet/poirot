use super::arguments::Arguments;
use crate::eval::functions::registry::InformationFunction;

/// Bound value-inspection call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct InformationExpr {
    /// Selected information handler.
    pub function: InformationFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
