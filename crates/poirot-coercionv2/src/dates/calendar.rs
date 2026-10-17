/// Civil date components used by calculation calendar conversions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    /// Civil year.
    pub y: i64,
    /// One-based month.
    pub m: i64,
    /// One-based day of month.
    pub d: i64,
}
/// Whether the Gregorian year has a leap day.
pub fn leap(y: i64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
/// Count days in a Gregorian month.
pub fn month_days(y: i64, m: i64) -> i64 {
    match m {
        2 => 28 + i64::from(leap(y)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
// Gregorian epoch algorithms, using 400-year eras (days relative to 1970-01-01).
/// Convert a civil date to days relative to 1970-01-01.
pub fn epoch(d: Date) -> i64 {
    let y = d.y - i64::from(d.m <= 2);
    let era = y.div_euclid(400);
    let yo = y - era * 400;
    let mp = d.m + if d.m > 2 { -3 } else { 9 };
    era * 146097 + yo * 365 + yo / 4 - yo / 100 + (153 * mp + 2) / 5 + d.d - 1 - 719468
}
/// Convert epoch-relative days to Gregorian components.
pub fn civil(days: i64) -> Date {
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
