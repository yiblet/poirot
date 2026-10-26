//! Reference family host integration.

/// Reference-valued builtin execution and dynamic selection.
pub(in crate::eval) mod execution;
mod metadata;

use crate::eval::host::CallHost;
use crate::eval::*;

/// Route reference helpers and dynamic selection through checked call services.
pub(in crate::eval) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: ReferenceFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if matches!(
        function,
        ReferenceFunction::Address
            | ReferenceFunction::Areas
            | ReferenceFunction::Hyperlink
            | ReferenceFunction::Sheet
            | ReferenceFunction::Rows
            | ReferenceFunction::Columns
            | ReferenceFunction::Row
            | ReferenceFunction::Column
    ) {
        if !FunctionId::Reference(function)
            .descriptor()
            .arity
            .accepts(arguments.len())
        {
            return Ok(err(VALUE_ERROR));
        }
        let mut host = CallHost::new(session, sheet, address, arguments);
        host.selection_builtin_policy();
        return match metadata::execute(&mut host, function) {
            Ok(value) => Ok(value),
            Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
            Err(FunctionFailure::Operational(failure)) => Err(failure),
        };
    }
    if let Some(helper) = function.helper() {
        let mut host = CallHost::new(session, sheet, address, arguments);
        host.selection_builtin_policy();
        if helper.policy().volatile {
            host.record_volatile(FunctionId::Reference(function));
        }
        return adapters::lookup::execute_host(host, helper);
    }
    execution::execute(
        &mut CallHost::new(session, sheet, address, arguments),
        function,
    )
}
