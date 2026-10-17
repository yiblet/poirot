//! Admission rules for calendar interval arguments.
use crate::CellError;
use crate::date_time::CalendarEpoch;

/// DATEDIF's explicit interval request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateIntervalUnit {
    /// Completed years.
    Years,
    /// Completed months.
    Months,
    /// Serial days.
    Days,
    /// Days ignoring months and years.
    MonthDays,
    /// Months ignoring years.
    YearMonths,
    /// Days ignoring years.
    YearDays,
}
/// Admit a case-insensitive unit without whitespace normalization.
pub fn coercev2_datedif_unit(unit: &str) -> Result<DateIntervalUnit, CellError> {
    for (name, value) in [
        ("Y", DateIntervalUnit::Years),
        ("M", DateIntervalUnit::Months),
        ("D", DateIntervalUnit::Days),
        ("MD", DateIntervalUnit::MonthDays),
        ("YM", DateIntervalUnit::YearMonths),
        ("YD", DateIntervalUnit::YearDays),
    ] {
        if unit.eq_ignore_ascii_case(name) {
            return Ok(value);
        }
    }
    Err(CellError::Num)
}
/// Admit a bounded serial before truncating its time fraction.
pub fn coercev2_datedif_day(serial: f64, epoch: CalendarEpoch) -> Result<i64, CellError> {
    let limit = match epoch {
        CalendarEpoch::Excel1900 => 2_958_466.,
        CalendarEpoch::Excel1904 => 2_957_004.,
    };
    if !serial.is_finite() || serial < 0. || serial >= limit {
        return Err(CellError::Num);
    }
    Ok(serial.floor() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_interval_requests_validate_before_truncation() {
        for epoch in [CalendarEpoch::Excel1900, CalendarEpoch::Excel1904] {
            assert_eq!(coercev2_datedif_day(1.9, epoch), Ok(1));
            for value in [-0.1, f64::INFINITY, f64::NAN, 2_958_466.] {
                assert_eq!(coercev2_datedif_day(value, epoch), Err(CellError::Num));
            }
        }
        assert_eq!(coercev2_datedif_unit("mD"), Ok(DateIntervalUnit::MonthDays));
        for unit in [" MD", "MD ", "", "N", "1"] {
            assert_eq!(coercev2_datedif_unit(unit), Err(CellError::Num));
        }
        assert_eq!(
            coercev2_datedif_day(2_957_004., CalendarEpoch::Excel1904),
            Err(CellError::Num)
        );
        assert_eq!(
            coercev2_datedif_day(2_958_465.9, CalendarEpoch::Excel1900),
            Ok(2_958_465)
        );
    }
}
