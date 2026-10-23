//! ISO weeks and inclusive business-day lifting with a shared holiday set.
use super::*;
use crate::adapters::{elementwise::Lift, failure::operational_only};
use crate::host::{CallHost, OperandOrigin};
use poirot_coercionv2::{
    Origin,
    collections::{HolidayPolicy, WeekendConsumer, coercev2_holiday_member, coercev2_weekend_mask},
    date_time::CalendarEpoch,
};

fn holidays(host: &mut CallHost<'_, '_>) -> FunctionResult<Vec<i64>> {
    let mut holidays = Vec::new();
    if host.len() < 4 {
        return Ok(holidays);
    }
    let count = host.collection_len(3)?;
    host.check_values(count)?;
    host.charge_work(count as u64)?;
    holidays
        .try_reserve_exact(count)
        .map_err(|_| host.allocation_failure())?;
    for offset in 0..count {
        let member = host.collection_cell(3, offset)?;
        if member.missing {
            continue;
        }
        let origin = match member.origin {
            OperandOrigin::Direct => Origin::Direct,
            OperandOrigin::ArrayElement => Origin::Array,
            OperandOrigin::ReferencedCell => Origin::Reference,
        };
        let day = coercev2_holiday_member(
            (&member.value).into(),
            origin,
            HolidayPolicy::NetworkDays,
            || host.coercev2_calendar_date(&member.value),
        )?;
        if let Some(day) = day {
            // SAFETY: The local count reservation bounds at most one push for each of count members.
            holidays.push(day.trunc() as i64);
        }
    }
    // SAFETY: i64's total comparison cannot panic; sort allocates no auxiliary storage.
    holidays.sort_unstable();
    holidays.dedup();
    Ok(holidays)
}
/// Execute ISO week numbers and customizable business-day counts.
pub(super) fn execute(
    session: &mut Session<'_>,
    sheet: usize,
    address: CellAddress,
    function: DateFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    if !FunctionId::Date(function)
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
        let lift = Lift::new(&mut host, args.len().min(3))?;
        let holidays = holidays(&mut host)?;
        lift.evaluate(&mut host, address, |host, row, column| {
            let first = lift.member(host, 0, row, column)?;
            let start = host.coercev2_calendar_date(&first.value)?.trunc() as i64;
            if function == DateFunction::IsoWeekNum {
                return Ok(Number(poirot_functions_dates::iso_week(start, epoch)?));
            }
            let second = lift.member(host, 1, row, column)?;
            let end = host.coercev2_calendar_date(&second.value)?.trunc() as i64;
            let weekends = if args.len() < 3 {
                weekend_policy(1)?
            } else {
                let member = lift.member(host, 2, row, column)?;
                if member.missing {
                    weekend_policy(1)?
                } else if let Some(text) = member.value.text_ref() {
                    let text = host.project_text(text)?;
                    coercev2_weekend_mask(&text, WeekendConsumer::NetworkDays)?
                } else {
                    let code = host.coercev2_scalar_number(
                        &member.value,
                        crate::host::NumericPolicy::PinnedPoi,
                    )?;
                    weekend_policy(coercev2_date_integer(code)?)?
                }
            };
            host.charge_work(holidays.len() as u64)?;
            Ok(Number(poirot_functions_dates::network_days(
                start, end, epoch, &weekends, &holidays,
            )?))
        })
    })();
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(error) => Err(operational_only(error)),
    }
}
