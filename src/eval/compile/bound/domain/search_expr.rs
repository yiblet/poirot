use super::arguments::Arguments;
use crate::eval::functions::registry::SearchFunction;

/// Bound lookup call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchExpr {
    /// Selected lookup handler.
    pub function: SearchFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
