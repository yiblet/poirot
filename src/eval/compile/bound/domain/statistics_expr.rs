use super::arguments::Arguments;
use crate::eval::functions::registry::StatisticsFunction;

/// Bound statistical call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct StatisticsExpr {
    /// Selected statistical handler.
    pub function: StatisticsFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
