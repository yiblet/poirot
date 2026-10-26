//! Logical functions use shared kernels and deferred selection.
use crate::eval::host::CallHost;
use crate::eval::*;

/// Execute logical tests and selected branches, preserving operational failures.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    at: CellAddress,
    function: LogicalFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    use LogicalFunction::*;
    let arity = FunctionId::Logical(function).descriptor().arity;
    if !arity.accepts(args.len()) {
        return Ok(err(VALUE_ERROR));
    }
    match function {
        If => super::manual::execute_if(session, sheet, at, args),
        Ifs | Switch | Choose => {
            let mut host = CallHost::new_inheriting(session, sheet, at, args);
            super::manual::execute_variadic(&mut host, function)
        }
        And | Or | Xor => super::aggregate::execute(session, sheet, at, function, args),
        _ => {
            let mut host = CallHost::new_inheriting(session, sheet, at, args);
            match semantic(super::modern::execute(&mut host, at, function))? {
                Ok(value) => Ok(value),
                Err(error) => Ok(err(error)),
            }
        }
    }
}
