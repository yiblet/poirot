use super::{execution, metadata};
use crate::eval::adapters;
use crate::eval::functions::registry::{FunctionId, ReferenceFunction};
use crate::eval::host::{CallHost, ContextReads};
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionError, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellAddress;

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
        ReferenceFunction::Cell
            | ReferenceFunction::Address
            | ReferenceFunction::Areas
            | ReferenceFunction::Hyperlink
            | ReferenceFunction::Sheet
            | ReferenceFunction::Sheets
            | ReferenceFunction::FormulaText
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

        return match metadata::execute(&mut host, function) {
            Ok(value) => Ok(value),
            Err(FunctionError::Spreadsheet(error)) => Ok(err(error)),
            Err(FunctionError::Operational(failure)) => Err(failure),
        };
    }
    if let Some(helper) = function.helper() {
        let mut host = CallHost::new(session, sheet, address, arguments);

        if helper.policy().volatile {
            ContextReads::new(&mut host).record_volatile(FunctionId::Reference(function));
        }
        return adapters::lookup::execute_host(host, helper);
    }
    execution::execute(
        &mut CallHost::new(session, sheet, address, arguments),
        function,
    )
}
