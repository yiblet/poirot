//! Search family host integration.

/// Lookup admission, sparse candidate traversal, and result projection.
pub(crate) mod execution;

/// Source-ordered classic lookup handler.
mod handler;

use crate::*;

/// Route search helpers and classic calls to their family-owned handlers.
pub(crate) fn execute(
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
