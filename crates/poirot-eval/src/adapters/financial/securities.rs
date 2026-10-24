//! Date-aware rectangular lifting for documented security calculations.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::CallHost;
use crate::*;
use poirot_coercionv2::date_time::CalendarEpoch;
use poirot_functions_financial::{
    FinancialError,
    modern::Failure,
    securities::{
        self as kernel, Basis, Bond, Day, DiscountOperation as Discount, Frequency,
        MaturityOperation as Maturity, TreasuryOperation as Treasury,
    },
};

fn spreadsheet(error: FinancialError) -> FunctionFailure {
    match error {
        FinancialError::Num => Num,
        FinancialError::Value => VALUE_ERROR,
        FinancialError::DivZero => Div0,
    }
    .into()
}
/// Admit a lifted date operand while preserving host calendar context and operand errors.
pub(super) fn date(
    host: &mut CallHost<'_, '_>,
    lift: &Lift,
    index: usize,
    row: usize,
    column: usize,
    epoch: CalendarEpoch,
) -> FunctionResult<Day> {
    let operand = lift.member(host, index, row, column)?;
    if operand.missing {
        return Err(VALUE_ERROR.into());
    }
    // Errors from the operand survive; only invalid calendar admission maps to VALUE.
    if let Error(error) = operand.value {
        return Err(error.into());
    }
    let value = host
        .coercev2_calendar_date(&operand.value)
        .map_err(|error| match error {
            FunctionFailure::Spreadsheet(_) => FunctionFailure::Spreadsheet(VALUE_ERROR),
            other => other,
        })?;
    Day::from_serial(value, epoch).map_err(spreadsheet)
}
/// Identify the security functions handled by this adapter.
pub(super) fn handles(function: FinancialFunction) -> bool {
    use FinancialFunction::*;
    matches!(
        function,
        Accrint
            | Accrintm
            | Coupdaybs
            | Coupdays
            | Coupdaysnc
            | Coupncd
            | Coupnum
            | Couppcd
            | Disc
            | Duration
            | Intrate
            | Mduration
            | Price
            | Pricedisc
            | Pricemat
            | Received
            | Tbilleq
            | Tbillprice
            | Tbillyield
            | Yield
            | Yielddisc
            | Yieldmat
    )
}
/// Coerce security inputs, publish lifted outputs and retain operational work failures.
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
            let first = date(host, &lift, 0, row, column, epoch)?;
            let second = date(host, &lift, 1, row, column, epoch)?;
            let mut n = |index, default| lift.number(host, index, row, column, default);
            let value = match function {
                Coupdaybs | Coupdays | Coupdaysnc | Coupncd | Coupnum | Couppcd => {
                    let frequency = Frequency::from_number(n(2, None)?).map_err(spreadsheet)?;
                    let basis = Basis::from_number(n(3, Some(0.0))?).map_err(spreadsheet)?;
                    let period = kernel::coupon_period(first, second, frequency, basis)
                        .map_err(spreadsheet)?;
                    match function {
                        Coupdaybs => period.accrued,
                        Coupdays => period.denominator,
                        Coupdaysnc => period.remaining,
                        Coupncd => period.next.serial(epoch),
                        Coupnum => f64::from(period.count),
                        _ => period.previous.serial(epoch),
                    }
                }
                Price | Yield | Duration | Mduration => {
                    let rate = n(2, None)?;
                    let value = n(3, None)?;
                    let duration = matches!(function, Duration | Mduration);
                    let redemption = if duration { 100.0 } else { n(4, None)? };
                    let frequency = Frequency::from_number(n(if duration { 4 } else { 5 }, None)?)
                        .map_err(spreadsheet)?;
                    let basis = Basis::from_number(n(if duration { 5 } else { 6 }, Some(0.0))?)
                        .map_err(spreadsheet)?;
                    let bond = Bond::new(first, second, rate, redemption, frequency, basis)
                        .map_err(spreadsheet)?;
                    match function {
                        Price => bond.price(value).map_err(spreadsheet)?,
                        Duration => bond.duration(value).map_err(spreadsheet)?,
                        Mduration => bond.modified_duration(value).map_err(spreadsheet)?,
                        _ => bond
                            .yield_rate(value, |work| host.charge_work(work))
                            .map_err(|error| match error {
                                Failure::Spreadsheet(error) => spreadsheet(error),
                                Failure::Operational(error) => error.into(),
                                Failure::ComputationalLimit => host.unsupported(
                                    CapabilityId::FunctionAdapter,
                                    "bond yield exceeds its numerical envelope",
                                ),
                            })?,
                    }
                }
                Accrint => {
                    let settlement = date(host, &lift, 2, row, column, epoch)?;
                    let rate = lift.number(host, 3, row, column, None)?;
                    let par = lift.number(host, 4, row, column, Some(1000.0))?;
                    let frequency =
                        Frequency::from_number(lift.number(host, 5, row, column, None)?)
                            .map_err(spreadsheet)?;
                    let basis = Basis::from_number(lift.number(host, 6, row, column, Some(0.0))?)
                        .map_err(spreadsheet)?;
                    let from_issue = if args.len() < 8 {
                        true
                    } else {
                        let member = lift.member(host, 7, row, column)?;
                        member.missing || core_projection::coercev2_operand_boolean(&member.value)?
                    };
                    kernel::accrued_interest(
                        first,
                        second,
                        settlement,
                        rate,
                        par,
                        frequency,
                        basis,
                        from_issue,
                        |work| host.charge_work(work),
                    )
                    .map_err(|error| match error {
                        Failure::Spreadsheet(error) => spreadsheet(error),
                        Failure::Operational(error) => error.into(),
                        Failure::ComputationalLimit => host.unsupported(
                            CapabilityId::FunctionAdapter,
                            "accrued interest exceeds its quasi-coupon envelope",
                        ),
                    })?
                }
                Accrintm => kernel::accrued_at_maturity(
                    first,
                    second,
                    n(2, None)?,
                    n(3, Some(1000.0))?,
                    Basis::from_number(n(4, Some(0.0))?).map_err(spreadsheet)?,
                )
                .map_err(spreadsheet)?,
                Pricemat | Yieldmat => {
                    let issue = date(host, &lift, 2, row, column, epoch)?;
                    let rate = lift.number(host, 3, row, column, None)?;
                    let value = lift.number(host, 4, row, column, None)?;
                    let basis = Basis::from_number(lift.number(host, 5, row, column, Some(0.0))?)
                        .map_err(spreadsheet)?;
                    kernel::maturity_security(
                        first,
                        second,
                        issue,
                        rate,
                        value,
                        basis,
                        if function == Pricemat {
                            Maturity::Price
                        } else {
                            Maturity::Yield
                        },
                    )
                    .map_err(spreadsheet)?
                }
                Tbilleq | Tbillprice | Tbillyield => kernel::treasury(
                    first,
                    second,
                    n(2, None)?,
                    match function {
                        Tbilleq => Treasury::Equivalent,
                        Tbillprice => Treasury::Price,
                        _ => Treasury::Yield,
                    },
                )
                .map_err(spreadsheet)?,
                Disc | Intrate | Pricedisc | Received | Yielddisc => kernel::discount(
                    first,
                    second,
                    n(2, None)?,
                    n(3, None)?,
                    Basis::from_number(n(4, Some(0.0))?).map_err(spreadsheet)?,
                    match function {
                        Disc => Discount::Discount,
                        Pricedisc => Discount::Price,
                        Received => Discount::Received,
                        _ => Discount::Yield,
                    },
                )
                .map_err(spreadsheet)?,
                _ => return Err(VALUE_ERROR.into()),
            };
            Ok(Number(value))
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
