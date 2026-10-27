use super::arguments::Arguments;
use crate::eval::functions::registry::FinancialFunction;

/// Bound financial call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct FinancialExpr {
    /// Selected financial handler.
    pub function: FinancialFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
