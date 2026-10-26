//! POI DateUtil's concatenated optional formatter, declared en-US profile.
//! Optional sections commit on success and restore fields/input on failure;
//! there is no global choice/backtracking between the four appendPattern calls.
use super::calendar::{Date, civil, epoch, month_days};
use crate::scalar::{CellError, CellError::Value as VALUE_ERROR};
/// Source profile uses explicit English names and a fixed timezone.
/// Other locale/zone profiles require additional token tables/bridge policy.
#[derive(Clone, Copy)]
pub enum SourceProfile {
    /// English month names and a fixed UTC timezone.
    EnglishUtc,
}
// year/month/day/hour/clock-hour/minute/second/nanos/ampm
/// Parsed formatter fields accumulated across committed optional pattern sections.
#[derive(Clone, Copy, Default)]
pub struct Plan {
    fields: [Option<i64>; 9],
}
/// Resolved optional date and seconds of day after formatter field validation.
#[derive(Clone, Copy)]
pub struct Resolved {
    /// Resolved Gregorian date, including accepted midnight rollover.
    pub date: Option<Date>,
    /// Seconds since midnight, with subsecond fields discarded.
    pub seconds: u32,
}
impl Plan {
    /// Parse all concatenated source patterns while retaining optional-section commits.
    pub fn inspect(_profile: SourceProfile, input: &str) -> Result<Self, CellError> {
        let fields =
            poirot_parsing::formatter::parse_formatter_fields(input).map_err(|_| VALUE_ERROR)?;
        Ok(Self { fields })
    }
    /// Whether a month/day spelling requires a year from evaluation context.
    pub fn needs_year(self) -> bool {
        self.fields[0].is_none() && self.fields[1].is_some() && self.fields[2].is_some()
    }
    /// Validate parsed fields and supply the caller's default year when needed.
    pub fn resolve(self, default_year: i64) -> Result<Resolved, CellError> {
        let f = self.fields;
        let year = f[0].unwrap_or(default_year);
        if !(1..=999_999_999).contains(&year) {
            return Err(VALUE_ERROR);
        }
        let mut date = match (f[1], f[2]) {
            (Some(m), Some(d)) => {
                if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
                    return Err(VALUE_ERROR);
                }
                Some(Date {
                    y: year,
                    m,
                    d: d.min(month_days(year, m)),
                })
            }
            _ => None,
        };
        let am_hour = match (f[4], f[8]) {
            (Some(h), Some(a)) if (0..=12).contains(&h) => Some(h % 12 + a * 12),
            (Some(h), _) if !(0..=12).contains(&h) => return Err(VALUE_ERROR),
            _ => None,
        };
        if f[3].is_some() && am_hour.is_some() && f[3] != am_hour {
            return Err(VALUE_ERROR);
        }
        let hour = f[3].or(am_hour);
        let mut seconds = 0;
        let mut has_time = false;
        if let (Some(h), Some(m)) = (hour, f[5]) {
            let s = f[6].unwrap_or(0);
            let nanos = f[7].unwrap_or(0);
            if !(0..=24).contains(&h) || !(0..=59).contains(&m) || !(0..=59).contains(&s) {
                return Err(VALUE_ERROR);
            }
            if h == 24 {
                if m != 0 || s != 0 || nanos != 0 {
                    return Err(VALUE_ERROR);
                }
                date = date.map(|d| civil(epoch(d) + 1));
                if date.is_some_and(|d| d.y > 999_999_999) {
                    return Err(VALUE_ERROR);
                }
            } else {
                seconds = (h * 3600 + m * 60 + s) as u32;
            }
            // Resolver cross-checks remaining clock-hour and AMPM fields.
            let h = h % 24;
            if am_hour.is_none() && f[4].is_some_and(|c| c % 12 != h % 12)
                || f[8].is_some_and(|a| a != h / 12)
            {
                return Err(VALUE_ERROR);
            }
            has_time = true;
        }
        if !has_time && date.is_none() {
            return Err(VALUE_ERROR);
        }
        Ok(Resolved { date, seconds })
    }
}

#[cfg(test)]
#[path = "time_formatter_tests.rs"]
mod tests;
