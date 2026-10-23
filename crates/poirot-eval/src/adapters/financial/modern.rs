//! Modern scalar finance lifting with explicitly consumed schedule/date collections.
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::CallHost;
use crate::*;
use poirot_functions_financial::{
    FinancialError,
    modern::{self as kernel, DatedFlow, LoanComponent},
};

fn spreadsheet(error: FinancialError) -> FunctionFailure {
    match error {
        FinancialError::Num => Num,
        FinancialError::DivZero => Div0,
        FinancialError::Value => VALUE_ERROR,
    }
    .into()
}
fn numerical(host: &CallHost<'_, '_>, error: kernel::Failure<FunctionFailure>) -> FunctionFailure {
    match error {
        kernel::Failure::Spreadsheet(error) => spreadsheet(error),
        kernel::Failure::Operational(error) => error,
        kernel::Failure::ComputationalLimit => host.unsupported(
            CapabilityId::FunctionAdapter,
            "financial computation exceeds its bounded work envelope",
        ),
    }
}
fn schedule(host: &mut CallHost<'_, '_>) -> FunctionResult<Vec<f64>> {
    let count = host.collection_len(1)?;
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for offset in 0..count {
        host.charge_work(1)?;
        let operand = host.collection_cell(1, offset)?;
        if operand.missing {
            return Err(VALUE_ERROR.into());
        }
        let value = match operand.value {
            Number(n) => n,
            Blank => 0.0,
            Error(e) => return Err(e.into()),
            _ => return Err(VALUE_ERROR.into()),
        };
        // SAFETY: count members were reserved locally; the loop pushes exactly count.
        values.push(value);
    }
    Ok(values)
}
fn dated_flows(host: &mut CallHost<'_, '_>, first: usize) -> FunctionResult<Vec<DatedFlow>> {
    let count = host.collection_len(first)?;
    let date_index = first.checked_add(1).ok_or(VALUE_ERROR)?;
    if count == 0 || host.collection_len(date_index)? != count {
        return Err(Num.into());
    }
    host.check_values(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    let mut anchor = None;
    for offset in 0..count {
        host.charge_work(1)?;
        let operand = host.collection_cell(first, offset)?;
        if operand.missing {
            return Err(VALUE_ERROR.into());
        }
        let value = match operand.value {
            Number(n) => n,
            Blank => 0.0,
            Error(e) => return Err(e.into()),
            _ => return Err(VALUE_ERROR.into()),
        };
        let operand = host.collection_cell(date_index, offset)?;
        if operand.missing {
            return Err(VALUE_ERROR.into());
        }
        let date = match operand.value {
            Number(n) if n.is_finite() && n >= 0.0 => host
                .coercev2_calendar_date(&Number(n.trunc()))
                .map_err(|e| match e {
                    FunctionFailure::Spreadsheet(_) => FunctionFailure::Spreadsheet(VALUE_ERROR),
                    e => e,
                })?
                .trunc(),
            Error(e) => return Err(e.into()),
            _ => return Err(VALUE_ERROR.into()),
        };
        let first_date = *anchor.get_or_insert(date);
        if date < first_date {
            return Err(Num.into());
        }
        // SAFETY: count members were reserved locally; this loop pushes count.
        values.push(DatedFlow {
            value,
            days: date - first_date,
        });
    }
    Ok(values)
}
/// Evaluate documented financial functions without changing pinned helper behavior.
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
    let mut host = CallHost::new_array(session, sheet, address, args, args.len());
    let result = (|| {
        use FinancialFunction::*;
        let lift = match function {
            Fvschedule | Xnpv => Lift::new(&mut host, 1)?,
            Xirr => Lift::new_at(&mut host, 2, usize::from(args.len() == 3))?,
            _ => Lift::new(&mut host, args.len())?,
        };
        let schedule = if function == Fvschedule {
            Some(schedule(&mut host)?)
        } else {
            None
        };
        let flows = if matches!(function, Xirr | Xnpv) {
            Some(dated_flows(&mut host, usize::from(function == Xnpv))?)
        } else {
            None
        };
        lift.evaluate(&mut host, address, |host, row, column| {
            let mut n = |index, default| lift.number(host, index, row, column, default);
            let value = match function {
                Effect => kernel::effect(n(0, None)?, n(1, None)?).map_err(spreadsheet)?,
                Nominal => kernel::nominal(n(0, None)?, n(1, None)?).map_err(spreadsheet)?,
                Pduration => {
                    kernel::pduration(n(0, None)?, n(1, None)?, n(2, None)?).map_err(spreadsheet)?
                }
                Rri => kernel::rri(n(0, None)?, n(1, None)?, n(2, None)?).map_err(spreadsheet)?,
                Sln => kernel::sln(n(0, None)?, n(1, None)?, n(2, None)?).map_err(spreadsheet)?,
                Syd => kernel::syd(n(0, None)?, n(1, None)?, n(2, None)?, n(3, None)?)
                    .map_err(spreadsheet)?,
                Ispmt => kernel::ispmt(n(0, None)?, n(1, None)?, n(2, None)?, n(3, None)?)
                    .map_err(spreadsheet)?,
                Db => kernel::db(
                    n(0, None)?,
                    n(1, None)?,
                    n(2, None)?,
                    n(3, None)?,
                    n(4, Some(12.0))?,
                )
                .map_err(spreadsheet)?,
                Ddb => kernel::ddb(
                    n(0, None)?,
                    n(1, None)?,
                    n(2, None)?,
                    n(3, None)?,
                    n(4, Some(2.0))?,
                )
                .map_err(spreadsheet)?,
                Cumipmt | Cumprinc => kernel::cumulative(
                    if function == Cumipmt {
                        LoanComponent::Interest
                    } else {
                        LoanComponent::Principal
                    },
                    n(0, None)?,
                    n(1, None)?,
                    n(2, None)?,
                    n(3, None)?,
                    n(4, None)?,
                    n(5, None)?,
                )
                .map_err(spreadsheet)?,
                Vdb => {
                    let (cost, salvage, life, start, end, factor) = (
                        n(0, None)?,
                        n(1, None)?,
                        n(2, None)?,
                        n(3, None)?,
                        n(4, None)?,
                        n(5, Some(2.0))?,
                    );
                    let no_switch = if args.len() < 7 {
                        false
                    } else {
                        let operand = lift.member(host, 6, row, column)?;
                        !operand.missing
                            && core_projection::coercev2_operand_boolean(&operand.value)?
                    };
                    kernel::vdb(
                        kernel::Asset {
                            cost,
                            salvage,
                            life,
                        },
                        start,
                        end,
                        factor,
                        no_switch,
                        |work| host.charge_work(work),
                    )
                    .map_err(|error| numerical(host, error))?
                }
                Fvschedule => {
                    let principal = n(0, None)?;
                    let schedule = schedule.as_ref().ok_or(VALUE_ERROR)?;
                    host.charge_work(
                        (schedule.len() as u64)
                            .checked_mul(2)
                            .ok_or_else(|| host.size_overflow())?,
                    )?;
                    kernel::fvschedule(principal, schedule).map_err(spreadsheet)?
                }
                Xnpv => {
                    let rate = n(0, None)?;
                    let flows = flows.as_ref().ok_or(VALUE_ERROR)?;
                    host.charge_work(
                        (flows.len() as u64)
                            .checked_mul(3)
                            .ok_or_else(|| host.size_overflow())?,
                    )?;
                    kernel::xnpv(rate, flows).map_err(spreadsheet)?
                }
                Xirr => {
                    let guess = n(0, Some(0.1))?;
                    kernel::xirr(flows.as_ref().ok_or(VALUE_ERROR)?, guess, |work| {
                        host.charge_work(work)
                    })
                    .map_err(|error| numerical(host, error))?
                }
                _ => return Err(VALUE_ERROR.into()),
            };
            // All numerical kernels reject nonfinite results before publication.
            Ok(Number(value))
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
