use super::calendar::{max_serial, pinned_gregorian_weekday};
use crate::{CellError, CellError::Num};
use poirot_model::DateSystem;

/// Count NETWORKDAYS using the pinned calendar and holiday arithmetic.
pub(super) fn network(
    start: f64,
    raw_end: f64,
    system: DateSystem,
    weekends: &[bool; 7],
    holidays: &[f64],
) -> f64 {
    let day = start.floor() as i64;
    let end = raw_end.floor() as i64;
    let sign = 1;
    let (lo, hi) = (day.min(end), day.max(end));
    // Count seven-day blocks without expanding millions of calendar cells.
    let split = if system == DateSystem::Excel1900 {
        61.min(hi + 1).max(lo)
    } else {
        lo
    };
    let mut count = (lo..split)
        // SAFETY: rem_euclid(7) produces an index in 0..7; weekends has seven entries in its type.
        .filter(|x| !weekends[pinned_gregorian_weekday(*x, system).rem_euclid(7) as usize])
        .count() as i64;
    let length = hi - split + 1;
    count += length / 7 * 5;
    for x in 0..length % 7 {
        // SAFETY: rem_euclid(7) produces an index in 0..7; weekends has seven entries in its type.
        if !weekends[pinned_gregorian_weekday(split + x, system).rem_euclid(7) as usize] {
            count += 1;
        }
    }
    for &h in holidays {
        // SAFETY: rem_euclid(7) produces an index in 0..7; weekends has seven entries in its type.
        if h >= start
            && h <= raw_end
            // SAFETY: rem_euclid(7) produces an index in 0..7; weekends has seven entries in its type.
            && !weekends[pinned_gregorian_weekday(h.floor() as i64, system).rem_euclid(7) as usize]
        {
            count -= 1;
        }
    }
    ((sign * count) as f64 + raw_end.fract() - start.fract()).trunc()
}

/// Validated calendar bounds and exclusion rules for advancing workdays.
pub(super) struct WorkdayPlan<'a> {
    /// Workbook serial date system.
    pub(super) system: DateSystem,
    /// Forward or backward day increment.
    pub(super) direction: i64,
    /// First permitted Gregorian epoch day.
    pub(super) first_calendar_day: i64,
    /// Last permitted Gregorian epoch day.
    pub(super) last_calendar_day: i64,
    /// Workdays in a complete Gregorian week.
    pub(super) work_per_week: i64,
    /// Sunday-first flags identifying excluded weekdays.
    pub(super) weekends: &'a [bool; 7],
    /// Sorted excluded workbook serial days.
    pub(super) sorted_holidays: &'a [i64],
}
/// Progress through separately tracked workbook serial and Gregorian day coordinates.
#[derive(Clone, Copy)]
pub(super) struct WorkdayState {
    /// Current workbook serial day.
    pub(super) day: i64,
    /// Current Gregorian epoch day.
    pub(super) calendar_day: i64,
    /// Workdays still to consume.
    pub(super) count: i64,
}
/// Advance by complete safe weeks or one calendar day under exclusion rules.
pub(super) fn advance(
    state: WorkdayState,
    plan: &WorkdayPlan<'_>,
) -> Result<WorkdayState, CellError> {
    let WorkdayState {
        mut day,
        mut calendar_day,
        mut count,
    } = state;
    let WorkdayPlan {
        system,
        direction,
        first_calendar_day,
        last_calendar_day,
        work_per_week,
        weekends,
        sorted_holidays,
    } = *plan;
    // Skip complete Gregorian weeks before the next holiday. Calendar days
    // and the separately incremented holiday serial stay distinct, as in POI.
    let next_holiday = if direction > 0 {
        sorted_holidays
            .get(sorted_holidays.partition_point(|h| *h <= day))
            .copied()
            .unwrap_or(max_serial(system) + 1)
    } else {
        sorted_holidays
            .partition_point(|h| *h < day)
            .checked_sub(1)
            .and_then(|i| sorted_holidays.get(i))
            .copied()
            .unwrap_or(-1)
    };
    let calendar_room = if direction > 0 {
        last_calendar_day - calendar_day
    } else {
        calendar_day - first_calendar_day
    };
    let available = ((next_holiday - day).abs() - 1).min(calendar_room);
    let weeks = ((count - 1) / work_per_week).min(available / 7);
    if weeks > 0 {
        day += direction * weeks * 7;
        calendar_day += direction * weeks * 7;
        count -= weeks * work_per_week;
        if count == 0 {
            return Ok(WorkdayState {
                day,
                calendar_day,
                count,
            });
        }
    }
    day += direction;
    calendar_day += direction;
    if calendar_day < first_calendar_day || calendar_day > last_calendar_day {
        return Err(Num);
    }
    // SAFETY: rem_euclid(7) produces an index in 0..7; weekends has seven entries in its type.
    if !weekends[(calendar_day + 4).rem_euclid(7) as usize]
        && sorted_holidays.binary_search(&day).is_err()
    {
        count -= 1;
    }

    Ok(WorkdayState {
        day,
        calendar_day,
        count,
    })
}

// Seven-character strings retain numeric-code coercion in the Session adapter.
/// Decode supported numeric weekend modes into Sunday-first exclusion flags.
pub(super) fn weekend_policy(mode: i64) -> Result<[bool; 7], CellError> {
    let mut weekends = [false; 7];
    match mode {
        1..=7 => {
            let first = (mode + 5) % 7;
            // SAFETY: The match restricts mode to 1..=7 or 11..=17, and modulo seven bounds both indices in the seven-element local array.
            weekends[first as usize] = true;
            // SAFETY: The match restricts mode to 1..=7 or 11..=17, and modulo seven bounds both indices in the seven-element local array.
            weekends[((first + 1) % 7) as usize] = true;
        }
        // SAFETY: The match restricts mode to 1..=7 or 11..=17, and modulo seven bounds both indices in the seven-element local array.
        11..=17 => weekends[((mode - 11) % 7) as usize] = true,
        _ => return Err(Num),
    }
    Ok(weekends)
}

/// Count an inclusive signed interval in Excel's continuous serial calendar.
/// Holidays are sorted, deduplicated, integer serials admitted by the caller.
pub(super) fn network_serial(
    start: i64,
    end: i64,
    system: DateSystem,
    weekends: &[bool; 7],
    holidays: &[i64],
) -> Result<f64, CellError> {
    let epoch = match system {
        DateSystem::Excel1900 => poirot_coercionv2::date_time::CalendarEpoch::Excel1900,
        DateSystem::Excel1904 => poirot_coercionv2::date_time::CalendarEpoch::Excel1904,
    };
    poirot_functions_dates::network_days(start, end, epoch, weekends, holidays)
}
fn is_serial_workday(day: i64, system: DateSystem, weekends: &[bool; 7]) -> bool {
    let weekday = super::calendar::excel_serial_weekday(day, system).rem_euclid(7) as usize;
    // SAFETY: rem_euclid(7) bounds weekday to 0..7 and the array has seven entries.
    !weekends[weekday]
}
/// Advance in one serial coordinate, jumping whole weeks before a holiday or
/// calendar bound. Each call is charged as one CalendarStep by the host.
pub(super) fn advance_serial(
    day: i64,
    count: i64,
    direction: i64,
    system: DateSystem,
    weekends: &[bool; 7],
    holidays: &[i64],
) -> Result<(i64, i64), CellError> {
    let limit = max_serial(system);
    if !(0..=i64::MAX / 16).contains(&limit)
        || !(0..=limit).contains(&day)
        || !(1..=limit).contains(&count)
        || ![-1, 1].contains(&direction)
    {
        return Err(Num);
    }
    let work_per_week = weekends.iter().filter(|day| !**day).count() as i64;
    if work_per_week == 0 {
        return Err(Num);
    }
    let next_holiday = if direction > 0 {
        holidays
            .get(holidays.partition_point(|h| *h <= day))
            .copied()
            .unwrap_or(limit + 1)
    } else {
        holidays
            .partition_point(|h| *h < day)
            .checked_sub(1)
            .and_then(|i| holidays.get(i))
            .copied()
            .unwrap_or(-1)
    };
    if !(-1..=limit + 1).contains(&next_holiday) {
        return Err(Num);
    }
    // SAFETY: The local checks bound day/count/next_holiday to calendar-sized
    // integers and direction to +/-1. Each subsequent difference/product is
    // at most a small multiple of limit, far below i64::MAX.
    let room = if direction > 0 { limit - day } else { day };
    let available = ((next_holiday - day).abs() - 1).min(room);
    let weeks = ((count - 1) / work_per_week).min(available / 7);
    let next = day + direction * (weeks * 7 + 1);
    if !(0..=limit).contains(&next) {
        return Err(Num);
    }
    let remaining = count
        - weeks * work_per_week
        - i64::from(
            is_serial_workday(next, system, weekends) && holidays.binary_search(&next).is_err(),
        );
    Ok((next, remaining))
}

#[cfg(test)]
mod modern_tests {
    use super::*;
    #[test]
    fn serial_helpers_reject_unbounded_inputs_without_arithmetic_panics() {
        let weekends = [true, false, false, false, false, false, true];
        let system = DateSystem::Excel1900;
        for (start, end) in [(i64::MIN, 1), (0, i64::MAX), (-1, 0)] {
            assert_eq!(network_serial(start, end, system, &weekends, &[]), Err(Num));
        }
        for (day, count, direction, holidays) in [
            (i64::MIN, 1, 1, vec![]),
            (i64::MAX, 1, -1, vec![]),
            (1, i64::MIN, 1, vec![]),
            (1, i64::MAX, 1, vec![]),
            (1, 1, i64::MAX, vec![]),
            (1, 1, -1, vec![i64::MIN]),
            (1, 1, 1, vec![i64::MAX]),
        ] {
            assert_eq!(
                advance_serial(day, count, direction, system, &weekends, &holidays),
                Err(Num)
            );
        }
        assert_eq!(advance_serial(1, 1, 1, system, &[true; 7], &[]), Err(Num));
        assert_eq!(network_serial(1, 100, system, &[true; 7], &[]), Ok(0.));
    }
}
