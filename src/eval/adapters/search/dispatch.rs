use super::handler;
use crate::eval::functions::registry::SearchFunction;
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::Value;
use crate::eval::vm::ArgumentsRef;
use crate::eval::{adapters, host};
use crate::model::CellAddress;

/// Route search helpers and classic calls to their family-owned handlers.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: SearchFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let mut host = host::CallHost::new(session, sheet, address, arguments);
    if let Some(helper) = function.helper() {
        return adapters::lookup::execute_host(host, helper);
    }
    handler::execute(&mut host, function)
}
