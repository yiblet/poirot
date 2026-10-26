use super::calendar::max_serial;
use crate::{CellError, CellError::Num};
use poirot_model::DateSystem;

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
#[cfg(test)]
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
