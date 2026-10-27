use super::arguments::Arguments;
use crate::eval::functions::registry::DatabaseFunction;

/// Bound database call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct DatabaseExpr {
    /// Selected database handler.
    pub function: DatabaseFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
