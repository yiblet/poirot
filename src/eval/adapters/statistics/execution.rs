//! Family-owned statistical dispatch over checked host execution.
use crate::eval::functions::registry::StatisticsFunction;
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::Value;
use crate::eval::vm::ArgumentsRef;
use crate::model::CellAddress;

/// Dispatch statistical calls to typed scalar, sample, or aggregate strategies.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    s: usize,
    at: CellAddress,
    function: StatisticsFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if super::ets::handles(function) {
        return super::ets::execute(session, s, at, function, args);
    }
    if super::sample::handles(function) {
        return super::sample::execute(session, s, at, function, args);
    }
    if super::modern::handles(function) {
        return super::modern::execute(session, s, at, function, args);
    }
    use StatisticsFunction::{
        Average, Count, CountA, CountBlank, Max, Median, Min, Product, Stdev, StdevP, Sum, SumSq,
        Var, VarP,
    };
    match function {
        Sum | Average | Count | CountA | CountBlank | Min | Max | Product | SumSq | Stdev
        | StdevP | Var | VarP | Median => super::aggregate::execute(session, s, at, function, args),
        _ => super::typed::execute(session, s, at, function, args),
    }
}
