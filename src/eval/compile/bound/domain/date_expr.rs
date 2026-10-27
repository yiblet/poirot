use super::arguments::Arguments;
use crate::eval::functions::registry::DateFunction;

/// Bound date or time call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct DateExpr {
    /// Selected date handler.
    pub function: DateFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
