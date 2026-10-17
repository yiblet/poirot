use super::calendar::{
    Date, civil, date, epoch, excel_serial_weekday, month_days, pinned_gregorian_weekday, serial,
    valid,
};
use crate::{CellError, CellError::Num, DateFunction, VALUE_ERROR};
use poirot_model::DateSystem;

/// Construct documented Excel DATE semantics with month and day overflow.
pub(super) fn excel_date(x: i64, y: i64, z: i64, system: DateSystem) -> Result<f64, CellError> {
    if !(0..=9999).contains(&x) {
        return Err(Num);
    }
    let year = if x < 1900 { x + 1900 } else { x };
    let total = year * 12 + y - 1;
    let first = Date {
        y: total.div_euclid(12),
        m: total.rem_euclid(12) + 1,
        d: 1,
    };
    // Normalize the month first, then add days in serial space so that the
    // fictitious 1900-02-29 participates in both forward and backward overflow.
    valid(
        (super::calendar::serial_day(first, system) + z - 1) as f64,
        system,
    )
}

/// Compute the ISO week number from a Gregorian date.
pub(super) fn iso_week(d: Date) -> f64 {
    let day = epoch(d);
    let monday_offset = (day + 3).rem_euclid(7);
    let thursday = day + 3 - monday_offset;
    let year = civil(thursday).y;
    let jan4 = epoch(Date {
        y: year,
        m: 1,
        d: 4,
    });
    let first_monday = jan4 - (jan4 + 3).rem_euclid(7);
    ((day - first_monday).div_euclid(7) + 1) as f64
}

/// Construct DATE or TIME using pinned source normalization and error rules.
// TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
pub(super) fn coerce_numeric_outcome(
    name: DateFunction,
    x: i64,
    y: i64,
    z: i64,
    system: DateSystem,
) -> Result<f64, CellError> {
    if name == DateFunction::Time {
        if x > 32767 || y > 32767 || z > 32767 {
            return Err(VALUE_ERROR);
        }
        let total = x * 3600 + y * 60 + z;
        if total < 0 {
            return Err(VALUE_ERROR);
        }
        return Ok((total % 86400) as f64 / 86400.);
    }
    if x < 0 {
        return Err(VALUE_ERROR);
    }
    let year = if x < 1900 { x + 1900 } else { x };
    let total = year * 12 + y - 1;
    let year = total.div_euclid(12);
    let month = total.rem_euclid(12) + 1;
    if !(1..=9999).contains(&year) {
        return Err(Num);
    }
    if system == DateSystem::Excel1900 && year == 1900 && month == 2 && z == 29 {
        return Ok(60.);
    }
    let adjusted = z - i64::from(
        system == DateSystem::Excel1900
            && year == 1900
            && ((month == 1 && z >= 60) || (month == 2 && z >= 30)),
    );
    let mut normalized = civil(
        epoch(Date {
            y: year,
            m: month,
            d: 1,
        }) + adjusted
            - 1,
    );
    if system == DateSystem::Excel1900
        && z < 0
        && normalized.y == 1900
        && month > 2
        && normalized.m < 3
    {
        normalized = civil(epoch(normalized) + 1);
    }
    serial(normalized, system)
}

/// Extract date or time components with pinned rounding and serial-zero behavior.
pub(super) fn component(
    name: DateFunction,
    start: f64,
    d: Date,
    system: DateSystem,
) -> Result<f64, CellError> {
    let sec = ((start.fract() * 86400.) + 0.4995).floor() as i64;
    let zero = system == DateSystem::Excel1900 && start.floor() == 0.;
    let d = if sec >= 86400 && !zero {
        date(start.floor() + 1., system)?
    } else {
        d
    };
    Ok(match name {
        DateFunction::Year => {
            if zero {
                1900
            } else {
                d.y
            }
        }
        DateFunction::Month => {
            if zero {
                1
            } else {
                d.m
            }
        }
        DateFunction::Day => {
            if zero {
                0
            } else {
                d.d
            }
        }
        DateFunction::Hour => sec / 3600 % 24,
        DateFunction::Minute => sec / 60 % 60,
        _ => sec % 60,
    } as f64)
}

/// Compute WEEKDAY or WEEKNUM under the selected return-type mode.
pub(super) fn week(
    name: DateFunction,
    start: f64,
    d: Date,
    mode: i64,
    system: DateSystem,
) -> Result<f64, CellError> {
    if name == DateFunction::WeekDay {
        return weekday_mode(pinned_gregorian_weekday(start.floor() as i64, system), mode);
    }
    let result = {
        let first = match mode {
            1 | 17 => 0,
            2 | 11 => 1,
            12..=16 => mode - 10,
            21 => 1,
            _ => return Err(Num),
        };
        let jan = epoch(Date { y: d.y, m: 1, d: 1 });
        let day = epoch(d) - jan;
        let offset = (jan + 4 - first).rem_euclid(7);
        let begin = if mode == 21 && offset > 3 {
            7 - offset
        } else {
            -offset
        };
        (day - begin).div_euclid(7) + 1
    };
    Ok(result as f64)
}

/// Compute documented WEEKDAY without losing the Excel serial-calendar identity.
pub(super) fn excel_weekday(start: f64, mode: i64, system: DateSystem) -> Result<f64, CellError> {
    weekday_mode(excel_serial_weekday(start.floor() as i64, system), mode)
}

fn weekday_mode(w: i64, mode: i64) -> Result<f64, CellError> {
    let w = w.rem_euclid(7);
    Ok(match mode {
        1 => w + 1,
        2 => (w + 6) % 7 + 1,
        3 => (w + 6) % 7,
        11..=17 => (w - (mode - 10) % 7).rem_euclid(7) + 1,
        _ => return Err(Num),
    } as f64)
}

/// Shift a date for EDATE or EOMONTH, applying month-end bounds.
pub(super) fn month(
    name: DateFunction,
    start: f64,
    d: Date,
    months: i64,
    system: DateSystem,
) -> Result<f64, CellError> {
    let d = if name == DateFunction::EOMonth && system == DateSystem::Excel1900 && start < 1. {
        Date {
            y: 1900,
            m: 1,
            d: 1,
        }
    } else {
        d
    };
    let total = d.y * 12 + d.m - 1 + months;
    let y = total.div_euclid(12);
    let m = total.rem_euclid(12) + 1;
    if !(1..=9999).contains(&y) {
        return Err(Num);
    }
    let day = if name == DateFunction::EOMonth {
        month_days(y, m)
    } else {
        d.d.min(month_days(y, m))
    };
    valid(
        serial(Date { y, m, d: day }, system)?
            + if name == DateFunction::EDate {
                start.fract()
            } else {
                0.
            },
        system,
    )
}
