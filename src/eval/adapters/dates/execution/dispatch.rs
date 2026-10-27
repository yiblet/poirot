use super::modern;
use crate::eval::functions::registry::DateFunction;
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::Value;
use crate::eval::vm::ArgumentsRef;
use crate::model::CellAddress;

/// Execute date kernels through shared typed source admission and output traversal.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    s: usize,
    at: CellAddress,
    name: DateFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let mut host = crate::eval::host::CallHost::new_array(session, s, at, args, args.len());
    modern::execute(&mut host, name)
}
