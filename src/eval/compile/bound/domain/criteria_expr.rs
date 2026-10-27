use super::arguments::Arguments;
use crate::eval::functions::registry::CriteriaFunction;

/// Bound criteria aggregation call with deferred arguments.
#[derive(Clone, Debug, PartialEq)]
pub struct CriteriaExpr {
    /// Selected criteria handler.
    pub function: CriteriaFunction,
    /// Deferred call operands and spelling metadata.
    pub arguments: Arguments,
}
