//! POI DateUtil's concatenated optional formatter, declared en-US profile.
//! Optional sections commit on success and restore fields/input on failure;
//! there is no global choice/backtracking between the four appendPattern calls.
use super::calendar::{Date, civil, epoch, month_days};
use nom::{Parser, bytes::complete::take_while_m_n};
use poirot_functions_core::{CellError, CellError::Value as VALUE_ERROR};
const PATTERNS: [&str; 4] = [
    "[d[.] [MMMM][MMM][ yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[[yyyy ]d-[MMMM][MMM][-yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[M/dd[/yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[[yyyy/]M/dd][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
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
fn set(plan: &mut Plan, key: usize, value: i64) -> Result<(), ()> {
    let field = plan.fields.get_mut(key).ok_or(())?;
    if field.is_some_and(|old| old != value) {
        return Err(());
    }
    *field = Some(value);
    Ok(())
}
fn numeric(input: &str, width: usize, year: bool) -> Result<(&str, i64), ()> {
    let signed = year || width == 1;
    let (raw, sign) = if signed && input.starts_with('-') || year && input.starts_with('+') {
        // SAFETY: The sign prefix is one ASCII byte; digits is a nonempty ASCII run parsed from raw, so digits.len() - 1 is a UTF-8 boundary within raw.
        (&input[1..], &input[..1])
    } else {
        (input, "")
    };
    let max = if width == 1 || year { 19 } else { width };
    let result: nom::IResult<&str, &str> =
        take_while_m_n(width, max, |c: char| c.is_ascii_digit()).parse(raw);
    let (mut tail, digits) = result.map_err(|_| ())?;
    if year && ((sign == "+" && digits.len() <= 4) || (sign.is_empty() && digits.len() > 4)) {
        return Err(());
    }
    let mut value = digits.parse::<i128>().map_err(|_| ())?;
    if sign == "-" {
        if value == 0 {
            return Err(());
        }
        value = -value;
    }
    // NumberPrinterParser drops its final digit when the BigInteger cannot
    // fit a signed long, then leaves that digit for the following parser.
    if value < i128::from(i64::MIN) || value > i128::from(i64::MAX) {
        value /= 10;
        // SAFETY: The sign prefix is one ASCII byte; digits is a nonempty ASCII run parsed from raw, so digits.len() - 1 is a UTF-8 boundary within raw.
        tail = &raw[digits.len() - 1..];
    }
    Ok((tail, value as i64))
}
fn token<'a>(input: &'a str, letter: u8, width: usize, plan: &mut Plan) -> Result<&'a str, ()> {
    if letter == b'M' && width >= 3 {
        for (month, name) in MONTHS.iter().enumerate() {
            let name = if width == 3 {
                name.get(..3).ok_or(())?
            } else {
                name
            };
            if let Some(tail) = input.strip_prefix(name) {
                set(plan, 1, month as i64 + 1)?;
                return Ok(tail);
            }
        }
        return Err(());
    }
    if letter == b'a' {
        for (name, value) in [("AM", 0), ("PM", 1)] {
            if let Some(tail) = input.strip_prefix(name) {
                set(plan, 8, value)?;
                return Ok(tail);
            }
        }
        return Err(());
    }
    let key = match letter {
        b'y' => 0,
        b'M' => 1,
        b'd' => 2,
        b'H' => 3,
        b'h' => 4,
        b'm' => 5,
        b's' => 6,
        b'S' => 7,
        _ => return Err(()),
    };
    let (tail, mut value) = numeric(input, width, letter == b'y')?;
    if letter == b'S' {
        value *= 1_000_000;
    }
    set(plan, key, value)?;
    Ok(tail)
}
fn close(pattern: &[u8], mut index: usize) -> Result<usize, ()> {
    let mut depth = 1;
    while depth != 0 {
        match *pattern.get(index).ok_or(())? {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        };
        index += 1;
    }
    index.checked_sub(1).ok_or(())
}
fn section<'a>(
    mut input: &'a str,
    pattern: &[u8],
    mut index: usize,
    end: usize,
    plan: &mut Plan,
) -> Result<&'a str, ()> {
    if index > end || end > pattern.len() {
        return Err(());
    }
    while index < end {
        // SAFETY: The entry guard checks end <= pattern.len(); the outer and run-scanning loops check their cursors against end.
        let byte = pattern[index];
        if byte == b'[' {
            let finish = close(pattern, index + 1)?;
            let mut tentative = *plan;
            if let Ok(tail) = section(input, pattern, index + 1, finish, &mut tentative) {
                input = tail;
                *plan = tentative;
            }
            index = finish + 1;
        } else if byte.is_ascii_alphabetic() {
            let mut finish = index + 1;
            // SAFETY: The entry guard checks end <= pattern.len(); the outer and run-scanning loops check their cursors against end.
            while finish < end && pattern[finish] == byte {
                finish += 1;
            }
            input = token(input, byte, finish - index, plan)?;
            index = finish;
        } else {
            input = input.strip_prefix(byte as char).ok_or(())?;
            index += 1;
        }
    }
    Ok(input)
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
    pub fn inspect(_profile: SourceProfile, mut input: &str) -> Result<Self, CellError> {
        let mut plan = Self::default();
        for pattern in PATTERNS {
            input = section(input, pattern.as_bytes(), 0, pattern.len(), &mut plan)
                .map_err(|_| VALUE_ERROR)?;
        }
        if !input.is_empty() {
            return Err(VALUE_ERROR);
        }
        Ok(plan)
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
