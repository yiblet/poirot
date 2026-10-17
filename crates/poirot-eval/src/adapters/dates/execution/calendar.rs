use crate::{CellError, CellError::Num};
use poirot_format::{CalendarPolicy, PhantomDayPolicy};
use poirot_model::DateSystem;

/// Civil date components used by calculation calendar conversions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Date {
    /// Civil year.
    pub(super) y: i64,
    /// One-based month.
    pub(super) m: i64,
    /// One-based day of month.
    pub(super) d: i64,
}
/// Whether the Gregorian year has a leap day.
pub(super) fn leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
/// Count days in a Gregorian month.
pub(super) fn month_days(y: i64, m: i64) -> i64 {
    match m {
        2 => 28 + i64::from(leap(y)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
// Gregorian epoch algorithms, using 400-year eras (days relative to 1970-01-01).
/// Convert a civil date to days relative to 1970-01-01.
pub(super) fn epoch(d: Date) -> i64 {
    let y = d.y - i64::from(d.m <= 2);
    let era = y.div_euclid(400);
    let yo = y - era * 400;
    let mp = d.m + if d.m > 2 { -3 } else { 9 };
    era * 146097 + yo * 365 + yo / 4 - yo / 100 + (153 * mp + 2) / 5 + d.d - 1 - 719468
}
/// Convert epoch-relative days to Gregorian components.
pub(super) fn civil(days: i64) -> Date {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let de = z - era * 146097;
    let yo = (de - de / 1460 + de / 36524 - de / 146096) / 365;
    let mut y = yo + era * 400;
    let dy = de - (365 * yo + yo / 4 - yo / 100);
    let mp = (5 * dy + 2) / 153;
    let d = dy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    y += i64::from(m <= 2);
    Date { y, m, d }
}
/// Select calculation bounds and serial-60 handling for a workbook date system.
pub(super) fn calculation_policy(system: DateSystem) -> CalendarPolicy {
    CalendarPolicy {
        date_system: system,
        phantom_day: PhantomDayPolicy::GregorianMarch1,
    }
}
/// Return the largest supported calculation day serial.
pub(super) fn max_serial(system: DateSystem) -> i64 {
    calculation_policy(system).maximum_day()
}
/// Validate a serial against the calculation calendar's bounds.
pub(super) fn valid(n: f64, system: DateSystem) -> Result<f64, CellError> {
    calculation_policy(system).validate(n).map_err(|_| Num)?;
    Ok(n)
}
/// Convert civil components to a bounded workbook day serial.
pub(super) fn serial(d: Date, system: DateSystem) -> Result<f64, CellError> {
    valid(serial_day(d, system) as f64, system)
}
// Unbounded conversion permits DATE month/day overflow to return into range.
/// Convert components without bounds checks so DATE overflow can normalize into range.
pub(super) fn serial_day(d: Date, system: DateSystem) -> i64 {
    if system == DateSystem::Excel1904 {
        epoch(d) + 24107
    } else if d
        == (Date {
            y: 1900,
            m: 2,
            d: 29,
        })
    {
        60
    } else {
        let e = epoch(d);
        e + 25568
            + i64::from(
                e >= epoch(Date {
                    y: 1900,
                    m: 3,
                    d: 1,
                }),
            )
    }
}
/// Resolve a bounded serial into calculation date components.
pub(super) fn date(n: f64, system: DateSystem) -> Result<Date, CellError> {
    let d = calculation_policy(system).date(n).map_err(|_| Num)?;
    Ok(Date {
        y: d.year,
        m: i64::from(d.month),
        d: i64::from(d.day),
    })
}
// YEARFRAC numeric text deliberately bypasses the bounded calendar policy, as in
// pinned YearFrac.evaluateDateArg. Keep its Gregorian conversion separate so
// the shared display/calculation bounds do not erase that operand exception.
/// Resolve YEARFRAC's numeric-text serial outside the bounded calculation calendar.
pub(super) fn calendar_date(n: f64, system: DateSystem) -> Result<Date, CellError> {
    let n = coerce_date_integer(n.floor())?;
    if system == DateSystem::Excel1900 && n == 60 {
        return Ok(Date {
            y: 1900,
            m: 3,
            d: 1,
        });
    }
    Ok(civil(if system == DateSystem::Excel1904 {
        n - 24107
    } else {
        n - 25568 - i64::from(n > 60)
    }))
}

/// Return the Gregorian weekday with Sunday as zero, preserving serial-60 behavior.
pub(super) fn weekday(n: i64, system: DateSystem) -> i64 {
    // Sunday=0. The serial-60 discontinuity follows Gregorian conversion in POI.
    let e = if system == DateSystem::Excel1904 {
        n - 24107
    } else {
        n - 25568 - i64::from(n > 60)
    };
    (e + 4).rem_euclid(7)
}
/// Truncate a finite value within the source's signed 32-bit argument range.
pub(super) fn coerce_date_integer(n: f64) -> Result<i64, CellError> {
    if n.is_finite() && n >= f64::from(i32::MIN) && n <= f64::from(i32::MAX) {
        Ok(n.trunc() as i64)
    } else {
        Err(Num)
    }
}
