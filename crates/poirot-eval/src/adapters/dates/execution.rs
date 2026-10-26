//! Raw date serial calculations, separate from display formatting.
//! Calendar arithmetic uses a proleptic Gregorian calendar and explicitly models
//! Excel's 1900 leap-day slot. No machine-local timezone or locale is consulted.
use crate::*;
#[cfg(test)]
use poirot_model::DateSystem;
mod business_days;
use crate::calendar;
mod components;
mod day_count;
mod modern;
use business_days::weekend_policy;
use calendar::{Date, calendar_date, civil, coercev2_date_integer, date, max_serial, serial};
use components::{coerce_numeric_outcome, component, month, week};
use day_count::{days360, yearfrac};
use poirot_coercionv2::dates::text;
use poirot_coercionv2::dates::time;
use poirot_coercionv2::dates::time_formatter;
use poirot_coercionv2::dates::time_grammar;
use text::DateTextPlan;

/// Resolve date text with only the clock and date-order inputs it consumes.
fn coercev2_date_text(
    host: &mut crate::host::CallHost<'_, '_>,
    input: &str,
    system: poirot_model::DateSystem,
) -> Result<Date, CellError> {
    let plan = DateTextPlan::inspect(input)?;
    let order = if plan.reads_date_order {
        match host.date_order() {
            DateOrder::MonthDayYear => poirot_coercionv2::dates::DateOrder::MonthDayYear,
            DateOrder::DayMonthYear => poirot_coercionv2::dates::DateOrder::DayMonthYear,
        }
    } else {
        poirot_coercionv2::dates::DateOrder::MonthDayYear
    };
    plan.coercev2_resolve(order, || Ok(date(host.clock_serial()?, system)?.y))
}

/// Select the captured calendar year for time grammar defaults.
fn time_default_year(host: &mut crate::host::CallHost<'_, '_>) -> Result<i64, CellError> {
    let seconds = crate::clock::unix_seconds(host.as_of_time());
    let offset = host.utc_offset_seconds();
    if !(-86400..=86400).contains(&offset) {
        return Err(Num);
    }
    let days = ((seconds + f64::from(offset)) / 86400.).floor();
    if !(-25568. ..2_932_897.).contains(&days) {
        return Err(Num);
    }
    Ok(civil(days as i64).y)
}

/// Attempt the source formatter and parser together without forcing a default year.
fn time_value_stage(
    host: &mut crate::host::CallHost<'_, '_>,
    original: &str,
    normalized: &str,
) -> Result<Option<f64>, CellError> {
    let Ok(util) =
        time_formatter::Plan::inspect(time_formatter::SourceProfile::EnglishUtc, normalized)
    else {
        return Ok(None);
    };
    let Ok(parser) = time_grammar::DatePlan::inspect(original) else {
        return Ok(None);
    };
    let year = if util.needs_year() || parser.needs_year() {
        time_default_year(host)?
    } else {
        2000
    };
    let (Ok(util), Ok(date)) = (util.resolve(year), parser.resolve(year)) else {
        return Ok(None);
    };
    let addition = util.date.map_or(0., time::source_day);
    Ok(Some(time::fraction(
        addition,
        util.seconds,
        time::source_day(date),
    )))
}

/// Parse TIMEVALUE using the same two source stages and charged text lengths.
fn coercev2_time_text(
    host: &mut crate::host::CallHost<'_, '_>,
    input: &str,
) -> FunctionResult<f64> {
    host.check_text(input.len())?;
    let normalized = time::normalize(input);
    if let Some(value) = time_value_stage(host, input, &normalized)? {
        return Ok(value);
    }
    let dummy_bytes =
        input
            .len()
            .checked_add("1/01/2000 ".len())
            .ok_or_else(|| EvaluationFailure::Resource {
                kind: crate::ResourceFailureKind::SizeOverflow,
                capability: CapabilityId::DateParsing,
                source: None,
                detail: "TIMEVALUE dummy prefix length overflow".into(),
            })?;
    host.check_text(dummy_bytes)?;
    let mut dummy = String::new();
    dummy
        .try_reserve_exact(dummy_bytes)
        .map_err(|_| host.allocation_failure())?;
    // SAFETY: dummy_bytes reserves the checked sum of both appended byte lengths.
    dummy.push_str("1/01/2000 ");
    // SAFETY: dummy_bytes reserves the checked sum of both appended byte lengths.
    dummy.push_str(input);
    host.check_text(dummy.len())?;
    let normalized_dummy = time::normalize(&dummy);
    if let Some(value) = time_value_stage(host, &dummy, &normalized_dummy)? {
        return Ok(value);
    }
    let plan = time_grammar::DatePlan::inspect(input)?;
    let year = if plan.needs_year() {
        time_default_year(host)?
    } else {
        2000
    };
    Ok(plan.resolve(year).map(|_| 0.)?)
}

/// Execute date kernels through shared typed source admission and output traversal.
pub(crate) fn execute(
    session: &mut Session<'_>,
    s: usize,
    at: CellAddress,
    name: DateFunction,
    args: ArgumentsRef<'_>,
) -> EvalResult<Value> {
    let mut host = crate::host::CallHost::new_array(session, s, at, args, args.len());
    modern::execute(&mut host, name)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod time_tests;

#[cfg(test)]
mod time_dispatch_tests;

#[cfg(test)]
#[path = "execution/numeric_boolean_tests.rs"]
mod numeric_boolean_tests;
