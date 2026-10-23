//! Date-aware rectangular operands for irregular coupons and French depreciation.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::CallHost;
use crate::*;
use poirot_coercionv2::date_time::CalendarEpoch;
use poirot_functions_financial::{
    FinancialError,
    modern::Failure,
    odd::{self as kernel, Amortization, OddFirst, OddLast},
    securities::{Basis, Frequency},
};
fn spreadsheet(error: FinancialError) -> FunctionFailure {
    match error {
        FinancialError::Num => Num,
        FinancialError::Value => VALUE_ERROR,
        FinancialError::DivZero => Div0,
    }
    .into()
}
fn failure(host: &CallHost<'_, '_>, error: Failure<FunctionFailure>) -> FunctionFailure {
    match error {
        Failure::Spreadsheet(e) => spreadsheet(e),
        Failure::Operational(e) => e,
        Failure::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            "irregular financial calculation exceeds its work envelope",
        ),
    }
}
/// Identify the six date-aware irregular financial operations.
pub(super) fn handles(function: FinancialFunction) -> bool {
    use FinancialFunction::*;
    matches!(
        function,
        Amordegrc | Amorlinc | Oddfprice | Oddfyield | Oddlprice | Oddlyield
    )
}
/// Coerce rectangular inputs, retain date context reads, and publish lifted numerical results.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: FinancialFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Financial(function)
        .descriptor()
        .arity
        .accepts(args.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    session.read(DependencyRead::ContextField(run::ContextRead::DateSystem));
    let epoch = match session.workbook.date_system {
        poirot_model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
        poirot_model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
    };
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = (|| {
        let lift = Lift::new(&mut host, args.len())?;
        lift.evaluate(&mut host, address, |host, row, column| {
            use FinancialFunction::*;
            let mut date = |index| super::securities::date(host, &lift, index, row, column, epoch);
            let value = if matches!(function, Amordegrc | Amorlinc) {
                let purchased = date(1)?;
                let first = date(2)?;
                let cost = lift.number(host, 0, row, column, None)?;
                let salvage = lift.number(host, 3, row, column, None)?;
                let period = lift.number(host, 4, row, column, None)?;
                let rate = lift.number(host, 5, row, column, None)?;
                let basis = Basis::from_number(lift.number(host, 6, row, column, Some(0.0))?)
                    .map_err(spreadsheet)?;
                kernel::amortization(
                    cost,
                    purchased,
                    first,
                    salvage,
                    period,
                    rate,
                    basis,
                    if function == Amorlinc {
                        Amortization::Linear
                    } else {
                        Amortization::Declining
                    },
                    |work| host.charge_work(work),
                )
                .map_err(|e| failure(host, e))?
            } else {
                let settlement = date(0)?;
                let maturity = date(1)?;
                let issue = date(2)?;
                let first = if matches!(function, Oddfprice | Oddfyield) {
                    Some(date(3)?)
                } else {
                    None
                };
                let start = if first.is_some() { 4 } else { 3 };
                let rate = lift.number(host, start, row, column, None)?;
                let value = lift.number(host, start + 1, row, column, None)?;
                let redemption = lift.number(host, start + 2, row, column, None)?;
                let frequency =
                    Frequency::from_number(lift.number(host, start + 3, row, column, None)?)
                        .map_err(spreadsheet)?;
                let basis =
                    Basis::from_number(lift.number(host, start + 4, row, column, Some(0.0))?)
                        .map_err(spreadsheet)?;
                if let Some(first) = first {
                    let bond = OddFirst::new(
                        settlement,
                        maturity,
                        issue,
                        first,
                        rate,
                        redemption,
                        frequency,
                        basis,
                        |work| host.charge_work(work),
                    )
                    .map_err(|e| failure(host, e))?;
                    if function == Oddfprice {
                        bond.price(value).map_err(spreadsheet)?
                    } else {
                        bond.yield_rate(value, |work| host.charge_work(work))
                            .map_err(|e| failure(host, e))?
                    }
                } else {
                    let bond = OddLast::new(
                        settlement,
                        maturity,
                        issue,
                        rate,
                        redemption,
                        frequency,
                        basis,
                        |work| host.charge_work(work),
                    )
                    .map_err(|e| failure(host, e))?;
                    if function == Oddlprice {
                        bond.price(value).map_err(spreadsheet)?
                    } else {
                        bond.yield_rate(value).map_err(spreadsheet)?
                    }
                }
            };
            Ok(Number(value))
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(e)) => Ok(err(e)),
        Err(e) => Err(operational_only(e)),
    }
}
