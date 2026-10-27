use crate::eval::compile::bound::{Arguments, Expression};
use crate::eval::functions::registry::FunctionId;

/// Extract an already bound builtin call without evaluating operands.
#[cfg(test)]
pub(in crate::eval) fn builtin_call(expression: &Expression) -> Option<(FunctionId, &Arguments)> {
    match expression {
        Expression::Date(call) => Some((FunctionId::Date(call.function), &call.arguments)),
        Expression::Logical(call) => Some((FunctionId::Logical(call.function), &call.arguments)),
        Expression::Statistics(call) => {
            Some((FunctionId::Statistics(call.function), &call.arguments))
        }
        Expression::Reference(call) => {
            Some((FunctionId::Reference(call.function), &call.arguments))
        }
        Expression::Search(call) => Some((FunctionId::Search(call.function), &call.arguments)),
        Expression::Information(call) => {
            Some((FunctionId::Information(call.function), &call.arguments))
        }
        Expression::Math(call) => Some((FunctionId::Math(call.function), &call.arguments)),
        Expression::Text(call) => Some((FunctionId::Text(call.function), &call.arguments)),
        Expression::Financial(call) => {
            Some((FunctionId::Financial(call.function), &call.arguments))
        }
        Expression::Engineering(call) => {
            Some((FunctionId::Engineering(call.function), &call.arguments))
        }
        Expression::Matrix(call) => Some((FunctionId::Matrix(call.function), &call.arguments)),
        Expression::Criteria(call) => Some((FunctionId::Criteria(call.function), &call.arguments)),
        Expression::Database(call) => Some((FunctionId::Database(call.function), &call.arguments)),
        _ => None,
    }
}
