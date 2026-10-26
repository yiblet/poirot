//! Shared calculation calendar bounds and workbook serial conversion.
use crate::{CellError, CellError::Num};
use poirot_format::{CalendarPolicy, PhantomDayPolicy};
use poirot_model::DateSystem;

pub(crate) use poirot_coercionv2::dates::calendar::{Date, civil, epoch, leap, month_days};
/// Select calculation bounds and serial-60 handling for a workbook date system.
pub(crate) fn calculation_policy(system: DateSystem) -> CalendarPolicy {
    CalendarPolicy {
        date_system: system,
        phantom_day: PhantomDayPolicy::GregorianMarch1,
    }
}
/// Return the largest supported calculation day serial.
pub(crate) fn max_serial(system: DateSystem) -> i64 {
    calculation_policy(system).maximum_day()
}
/// Validate a serial against the calculation calendar's bounds.
pub(crate) fn valid(n: f64, system: DateSystem) -> Result<f64, CellError> {
    calculation_policy(system).validate(n).map_err(|_| Num)?;
    Ok(n)
}
/// Convert civil components to a bounded workbook day serial.
pub(crate) fn serial(d: Date, system: DateSystem) -> Result<f64, CellError> {
    valid(serial_day(d, system) as f64, system)
}
// Unbounded conversion permits DATE month/day overflow to return into range.
/// Convert components without bounds checks so DATE overflow can normalize into range.
pub(crate) fn serial_day(d: Date, system: DateSystem) -> i64 {
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
pub(crate) fn date(n: f64, system: DateSystem) -> Result<Date, CellError> {
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
pub(crate) fn calendar_date(n: f64, system: DateSystem) -> Result<Date, CellError> {
    let n = coercev2_date_integer(n.floor())?;
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
pub(crate) fn pinned_gregorian_weekday(n: i64, system: DateSystem) -> i64 {
    // Sunday=0. The serial-60 discontinuity follows Gregorian conversion in POI.
    let e = if system == DateSystem::Excel1904 {
        n - 24107
    } else {
        n - 25568 - i64::from(n > 60)
    };
    (e + 4).rem_euclid(7)
}

/// Return Excel's serial-calendar weekday with Sunday as zero.
pub(crate) fn excel_serial_weekday(n: i64, system: DateSystem) -> i64 {
    // Excel keeps a continuous seven-day cycle across its fictitious 1900-02-29.
    // Thus serial 1 is Sunday rather than Gregorian 1900-01-01's Monday.
    // The 1904 system starts on Friday and has no fictitious leap day.
    let origin = if system == DateSystem::Excel1904 {
        5
    } else {
        6
    };
    (n.rem_euclid(7) + origin) % 7
}
/// Truncate a finite value within the source's signed 32-bit argument range.
pub(crate) fn coercev2_date_integer(n: f64) -> Result<i64, CellError> {
    poirot_coercionv2::compatibility::coercev2_date_integer(n)
}
