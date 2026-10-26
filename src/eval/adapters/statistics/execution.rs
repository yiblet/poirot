//! Family-owned statistical dispatch over checked host execution.
use crate::eval::*;

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
    use StatisticsFunction::*;
    match function {
        Sum | Average | Count | CountA | CountBlank | Min | Max | Product | SumSq | Stdev
        | StdevP | Var | VarP | Median => super::aggregate::execute(session, s, at, function, args),
        _ => super::typed::execute(session, s, at, function, args),
    }
}
