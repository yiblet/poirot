//! Calculate calendar intervals with spreadsheet DATEDIF rules.
//!
//! Call [`datedif`](crate::dates::datedif) with two date serials, a `DateIntervalUnit`, and the workbook's
//! date system. It calculates complete years, months, days, and supported remainder
//! units, including the implemented Excel calendar quirks.
//!
//! Date parsing and argument conversion live in
//! [coercion](https://docs.rs/poirot-functions/latest/poirot_functions/coercion).
use crate::coercion::dates::calendar::{Date, civil, epoch};
use crate::coercion::{date_time::CalendarEpoch, dates::interval::DateIntervalUnit};
use crate::scalar::CellError;
mod business;
pub use business::{iso_week, network_days};

/// Compute completed calendar intervals from admitted date serials.
/// Unit text is admitted by the coercion module. MD preserves the historical borrowing
/// rule, including negative results when the preceding month is too short.
pub fn datedif(
    start: i64,
    end: i64,
    unit: DateIntervalUnit,
    epoch: CalendarEpoch,
) -> Result<f64, CellError> {
    let epoch_1904 = matches!(epoch, CalendarEpoch::Excel1904);
    let max = if epoch_1904 { 2_957_003 } else { 2_958_465 };
    if start < 0 || end < start || end > max {
        return Err(CellError::Num);
    }
    let a = components(start, epoch_1904)?;
    let b = components(end, epoch_1904)?;
    for date in [a, b] {
        if !(1900..=9999).contains(&date.y)
            || !(1..=12).contains(&date.m)
            || !(0..=31).contains(&date.d)
        {
            return Err(CellError::Num);
        }
    }
    // SAFETY: Both dates were locally checked to years 1900..=9999, months
    // 1..=12 and days 0..=31; their differences, year/month shifts and products
    // by twelve stay below 120000 in magnitude. Admitted serial differences
    // and calendar-projection differences stay below 4000000, far inside i64.
    let anniversary = (b.m, b.d) >= (a.m, a.d);
    let months = (b.y - a.y) * 12 + b.m - a.m - i64::from(b.d < a.d);
    let n = match unit {
        DateIntervalUnit::Years => b.y - a.y - i64::from(!anniversary),
        DateIntervalUnit::Months => months,
        DateIntervalUnit::Days => end - start,
        DateIntervalUnit::YearMonths => months.rem_euclid(12),
        DateIntervalUnit::MonthDays => {
            if b.d >= a.d {
                b.d - a.d
            } else {
                let preceding = Date {
                    y: b.y - i64::from(b.m == 1),
                    m: if b.m == 1 { 12 } else { b.m - 1 },
                    d: a.d,
                };
                end.checked_sub(serial(preceding, epoch_1904)?)
                    .ok_or(CellError::Num)?
            }
        }
        DateIntervalUnit::YearDays => {
            let adjacent = Date {
                y: a.y + i64::from(!anniversary),
                ..b
            };
            serial(adjacent, epoch_1904)?
                .checked_sub(start)
                .ok_or(CellError::Num)?
        }
    };
    // TODO(coercion-non-argument): [LOW] A future extension beyond bounded workbook dates could exceed exact f64 integer output.
    Ok(n as f64)
}

fn components(serial: i64, epoch_1904: bool) -> Result<Date, CellError> {
    if !(0..=2_958_465).contains(&serial) {
        return Err(CellError::Num);
    }
    // SAFETY: The local serial bound is 0..=2958465, so subtracting the
    // epoch offsets and at most one extra day stays within -25569..=2934358.
    Ok(if epoch_1904 {
        civil(serial - 24107)
    } else if serial == 0 {
        Date {
            y: 1900,
            m: 1,
            d: 0,
        }
    } else if serial == 60 {
        Date {
            y: 1900,
            m: 2,
            d: 29,
        }
    } else {
        civil(serial - 25568 - i64::from(serial > 60))
    })
}

fn serial(date: Date, epoch_1904: bool) -> Result<i64, CellError> {
    if !(1899..=10000).contains(&date.y)
        || !(1..=12).contains(&date.m)
        || !(0..=31).contains(&date.d)
    {
        return Err(CellError::Num);
    }
    // Normalize overflowing days from the first of the month, preserving the
    // phantom slot even for February 30/31 used by MD borrowing.
    let first = epoch(Date { d: 1, ..date });
    let offset = if epoch_1904 {
        24107
    } else {
        25568 + i64::from((date.y, date.m) >= (1900, 3))
    };
    first
        .checked_add(offset)
        .and_then(|base| base.checked_add(date.d))
        .and_then(|day| day.checked_sub(1))
        .ok_or(CellError::Num)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interval_kernel_rejects_unbounded_or_reversed_integer_inputs() {
        for epoch in [CalendarEpoch::Excel1900, CalendarEpoch::Excel1904] {
            for (start, end) in [(i64::MIN, 0), (0, i64::MAX), (2, 1), (-1, 1)] {
                assert_eq!(
                    datedif(start, end, DateIntervalUnit::Days, epoch),
                    Err(CellError::Num)
                );
            }
            assert_eq!(datedif(1, 1, DateIntervalUnit::MonthDays, epoch), Ok(0.));
        }
    }
}
