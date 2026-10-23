//! Bounded serial-calendar business days and Gregorian ISO week numbering.
use super::{CalendarEpoch, CellError, Date, civil, epoch};

/// Count an inclusive signed interval, excluding a Sunday-first weekend mask.
/// Holiday serials must be strictly increasing; invalid lists are rejected.
pub fn network_days(
    start: i64,
    end: i64,
    system: CalendarEpoch,
    weekends: &[bool; 7],
    holidays: &[i64],
) -> Result<f64, CellError> {
    let limit = if matches!(system, CalendarEpoch::Excel1904) {
        2_957_003
    } else {
        2_958_465
    };
    if !(0..=limit).contains(&start) || !(0..=limit).contains(&end) {
        return Err(CellError::Num);
    }
    if holidays.windows(2).any(|p| !matches!(p, [a,b] if a < b)) {
        return Err(CellError::Value);
    }
    // SAFETY: Both endpoints are checked within 0..=2,958,465. Interval lengths,
    // six residual offsets and counts remain below three million, inside i64.
    let (lo, hi) = (start.min(end), start.max(end));
    let length = hi - lo + 1;
    let work = |day: i64| {
        let origin = if matches!(system, CalendarEpoch::Excel1904) {
            5
        } else {
            6
        };
        let index = (day.rem_euclid(7) + origin) % 7;
        // SAFETY: Two nonnegative residues and a final modulo seven bound this index to 0..7.
        !weekends[index as usize]
    };
    let mut count = length / 7 * weekends.iter().filter(|b| !**b).count() as i64;
    for offset in 0..length % 7 {
        if work(lo + offset) {
            count += 1;
        }
    }
    for &holiday in holidays {
        if (lo..=hi).contains(&holiday) && work(holiday) {
            count = count.checked_sub(1).ok_or(CellError::Num)?;
        }
    }
    if start > end {
        count = count.checked_neg().ok_or(CellError::Num)?;
    }
    Ok(count as f64)
}

/// Gregorian ISO week number in the workbook's supported serial-date system.
pub fn iso_week(serial: i64, system: CalendarEpoch) -> Result<f64, CellError> {
    let date = super::components(serial, matches!(system, CalendarEpoch::Excel1904))?;
    if !(1899..=9999).contains(&date.y)
        || !(1..=12).contains(&date.m)
        || !(0..=31).contains(&date.d)
    {
        return Err(CellError::Num);
    }
    // SAFETY: The date's locally checked year/month/day fit the bounded Gregorian
    // epoch formula. Epoch days and a six-day shift are below four million.
    let day = epoch(date);
    let thursday = day + 3 - (day + 3).rem_euclid(7);
    let year = civil(thursday).y;
    if !(1899..=10000).contains(&year) {
        return Err(CellError::Num);
    }
    let jan4 = epoch(Date {
        y: year,
        m: 1,
        d: 4,
    });
    let first_monday = jan4 - (jan4 + 3).rem_euclid(7);
    Ok(((day - first_monday).div_euclid(7) + 1) as f64)
}
