//! Independent serial-calendar counting and Gregorian ISO boundaries.
use poirot_functions::coercion::date_time::CalendarEpoch as E;
use poirot_functions::dates::{iso_week, network_days};
use poirot_functions::scalar::CellError;
#[test]
fn serial_days_are_signed_deduplicated_and_calendar_bounded() {
    let weekends = [true, false, false, false, false, false, true];
    assert_eq!(
        network_days(45292, 45298, E::Excel1900, &weekends, &[45293, 45294]),
        Ok(3.)
    );
    assert_eq!(
        network_days(45298, 45292, E::Excel1900, &weekends, &[45293, 45294]),
        Ok(-3.)
    );
    assert_eq!(network_days(0, 60, E::Excel1900, &[false; 7], &[]), Ok(61.));
    assert_eq!(network_days(0, 60, E::Excel1900, &[true; 7], &[]), Ok(0.));
    assert_eq!(
        network_days(0, 60, E::Excel1900, &weekends, &[1, 1]),
        Err(CellError::Value)
    );
    assert_eq!(
        network_days(i64::MIN, i64::MAX, E::Excel1900, &weekends, &[]),
        Err(CellError::Num)
    );
    // Preserve the predecessor's continuous Excel weekday convention, including phantom day 60.
    assert_eq!(network_days(0, 0, E::Excel1900, &weekends, &[]), Ok(0.));
    assert_eq!(network_days(60, 60, E::Excel1900, &weekends, &[]), Ok(1.));
    assert_eq!(network_days(0, 0, E::Excel1904, &weekends, &[]), Ok(1.));
}
#[test]
fn iso_weeks_belong_to_the_first_thursday_year() {
    assert_eq!(iso_week(44197, E::Excel1900), Ok(53.));
    assert_eq!(iso_week(44200, E::Excel1900), Ok(1.));
    assert_eq!(iso_week(44197, E::Excel1904), Ok(1.));
    assert_eq!(iso_week(-1, E::Excel1900), Err(CellError::Num));
    assert_eq!(iso_week(i64::MAX, E::Excel1904), Err(CellError::Num));
}
