use crate::eval::config::DateOrder;
use crate::eval::config::calendar::date;
use crate::eval::host::ContextReads;
use crate::eval::limits::{CapabilityId, EvaluationFailure};
use crate::eval::values::FunctionResult;
use crate::model::CellError;
use crate::model::CellError::Num;
use poirot_functions::coercion::dates::calendar::{Date, civil};
use poirot_functions::coercion::dates::{text, time, time_formatter, time_grammar};
use text::DateTextPlan;

/// Resolve date text with only the clock and date-order inputs it consumes.
pub(super) fn coerce_date_text(
    host: &mut crate::eval::host::CallHost<'_, '_>,
    input: &str,
    system: crate::model::DateSystem,
) -> Result<Date, CellError> {
    let plan = DateTextPlan::inspect(input)?;
    let order = if plan.reads_date_order {
        match ContextReads::new(host).date_order() {
            DateOrder::MonthDayYear => poirot_functions::coercion::dates::DateOrder::MonthDayYear,
            DateOrder::DayMonthYear => poirot_functions::coercion::dates::DateOrder::DayMonthYear,
        }
    } else {
        poirot_functions::coercion::dates::DateOrder::MonthDayYear
    };
    plan.coerce_resolve(order, || {
        Ok(date(ContextReads::new(host).clock_serial()?, system)?.y)
    })
}

/// Select the captured calendar year for time grammar defaults.
pub(super) fn time_default_year(
    host: &mut crate::eval::host::CallHost<'_, '_>,
) -> Result<i64, CellError> {
    let seconds = crate::eval::config::clock::unix_seconds(ContextReads::new(host).as_of_time());
    let offset = ContextReads::new(host).utc_offset_seconds();
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
pub(super) fn time_value_stage(
    host: &mut crate::eval::host::CallHost<'_, '_>,
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
pub(super) fn coerce_time_text(
    host: &mut crate::eval::host::CallHost<'_, '_>,
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
                kind: crate::eval::limits::ResourceFailureKind::SizeOverflow,
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
