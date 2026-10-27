use super::{modern_complex, modern_radix, modern_special};
use crate::eval::functions::registry::{EngineeringFunction, FunctionId};
use crate::eval::host::CallHost;
use crate::eval::limits::EvalResult;
use crate::eval::session::Session;
use crate::eval::values::{FunctionFailure, VALUE_ERROR, Value, err};
use crate::eval::vm::ArgumentsRef;
use crate::model::CellAddress;

/// Admit and execute an engineering kernel through the shared typed executor.
pub(super) fn run<K: crate::eval::adapters::elementwise::Kernel>(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
    kernel: K,
) -> EvalResult<Value> {
    if !FunctionId::Engineering(function)
        .descriptor()
        .arity
        .accepts(arguments.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let mut host = CallHost::new_array(session, sheet, address, arguments, arguments.len());
    match crate::eval::adapters::elementwise::execute(&mut host, address, kernel) {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(crate::eval::adapters::failure::operational_only(error)),
    }
}

/// Dispatch modern engineering operations to typed scalar and collection kernels.
pub(in crate::eval) fn execute_function(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: EngineeringFunction,
    arguments: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if modern_special::handles(function) {
        modern_special::execute(session, sheet, address, function, arguments)
    } else if modern_radix::handles(function) {
        modern_radix::execute(session, sheet, address, function, arguments)
    } else {
        modern_complex::execute(session, sheet, address, function, arguments)
    }
}
