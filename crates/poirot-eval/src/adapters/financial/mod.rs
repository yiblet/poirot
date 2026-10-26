//! Typed financial kernels over deferred scalar and whole-collection roles.
use crate::*;
mod cashflow;
mod euro;
mod modern;
mod odd;
mod scalar;
mod securities;
/// Dispatch financial identities through shared typed-kernel execution.
pub(crate) fn execute_function(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: FinancialFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if function == FinancialFunction::Euroconvert {
        return euro::execute(session, sheet, address, args);
    }
    if odd::handles(function) {
        return odd::execute(session, sheet, address, function, args);
    }
    if securities::handles(function) {
        return securities::execute(session, sheet, address, function, args);
    }
    if matches!(
        function,
        FinancialFunction::Fv
            | FinancialFunction::Pv
            | FinancialFunction::Pmt
            | FinancialFunction::Nper
            | FinancialFunction::Ipmt
            | FinancialFunction::Ppmt
            | FinancialFunction::Rate
            | FinancialFunction::DollarDe
            | FinancialFunction::DollarFr
            | FinancialFunction::Fixed
            | FinancialFunction::Dollar
    ) {
        return scalar::execute(session, sheet, address, function, args);
    }
    if matches!(
        function,
        FinancialFunction::Npv | FinancialFunction::Irr | FinancialFunction::Mirr
    ) {
        cashflow::execute(session, sheet, address, function, args)
    } else {
        modern::execute(session, sheet, address, function, args)
    }
}
