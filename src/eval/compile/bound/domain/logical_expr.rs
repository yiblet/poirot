use super::arguments::Arguments;
use crate::eval::functions::registry::LogicalFunction;

/// Bound logical call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct LogicalExpr {
    /// Selected logical handler.
    pub function: LogicalFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
