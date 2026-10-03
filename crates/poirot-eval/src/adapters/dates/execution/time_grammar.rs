//! Bounded TIMEVALUE DateParser plan; independent of DATEVALUE's date order.
use super::calendar::{Date, month_days};
use crate::{CellError, VALUE_ERROR};
use nom::{IResult, Parser, bytes::complete::take_while_m_n, character::complete::char};

/// Inspected TIMEVALUE date components before optional default-year resolution.
#[derive(Clone, Copy)]
pub(super) struct DatePlan<'a> {
    year: Option<i64>,
    month: &'a str,
    day: i64,
}
fn digits(input: &str, min: usize, max: usize) -> IResult<&str, &str> {
    take_while_m_n(min, max, |c: char| c.is_ascii_digit()).parse(input)
}
fn word(input: &str) -> IResult<&str, &str> {
    take_while_m_n(1, input.len(), |c: char| {
        c.is_ascii_alphanumeric() || c == '_'
    })
    .parse(input)
}
fn number(s: &str) -> i64 {
    s.parse().unwrap_or(-1)
}
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
fn month(s: &str) -> i64 {
    if s.bytes().all(|b| b.is_ascii_digit()) {
        return number(s);
    }
    MONTHS
        .iter()
        .position(|m| m.get(..s.len()).is_some_and(|p| p.eq_ignore_ascii_case(s)))
        .map_or(-1, |n| n as i64 + 1)
}
// DateParser regex order: YMD dash, DMY dash, MD dash, MDY slash,
// YMD slash, MD slash. At most six bounded attempts, no field vector.
fn fields(input: &str, index: usize) -> IResult<&str, DatePlan<'_>> {
    let slash = index >= 3;
    let sep = if slash { '/' } else { '-' };
    let ymd = index == 0 || index == 4;
    let dmy = index == 1;
    let has_year = ymd || dmy || index == 3;
    let (input, a) = if ymd {
        digits(input, 4, 4)?
    } else if dmy {
        digits(input, 1, 2)?
    } else {
        word(input)?
    };
    let (input, _) = char(sep).parse(input)?;
    let (input, b) = if ymd || dmy {
        word(input)?
    } else {
        digits(input, 1, 2)?
    };
    let (input, c) = if has_year {
        let (input, _) = char(sep).parse(input)?;
        let (input, c) = digits(input, if ymd { 1 } else { 4 }, if ymd { 2 } else { 4 })?;
        (input, Some(c))
    } else {
        (input, None)
    };
    let (year, mon, day) = if ymd {
        // SAFETY: ymd or dmy makes the local has_year flag true, and its branch constructs c as Some before this access.
        (Some(number(a)), b, c.unwrap())
    } else if dmy {
        // SAFETY: ymd or dmy makes the local has_year flag true, and its branch constructs c as Some before this access.
        (Some(number(c.unwrap())), b, a)
    } else {
        (c.map(number), a, b)
    };
    Ok((
        input,
        DatePlan {
            year,
            month: mon,
            day: number(day),
        },
    ))
}
impl<'a> DatePlan<'a> {
    /// Parse TIMEVALUE's independent date grammar without consulting context.
    pub(super) fn inspect(input: &'a str) -> Result<Self, CellError> {
        for index in 0..6 {
            if let Ok((tail, plan)) = fields(input, index) {
                let tail = tail
                    .strip_suffix("\r\n")
                    .or_else(|| tail.strip_suffix(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']))
                    .unwrap_or(tail);
                if tail.is_empty()
                    || (tail.starts_with(' ')
                        && !tail.contains(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}']))
                {
                    return Ok(plan);
                }
            }
        }
        Err(VALUE_ERROR)
    }
    /// Whether the host must supply a captured current year.
    pub(super) fn needs_year(self) -> bool {
        self.year.is_none()
    }
    /// Resolve inspected fields with an explicit default year and Gregorian validation.
    pub(super) fn resolve(self, year: i64) -> Result<Date, CellError> {
        let date = Date {
            y: self.year.unwrap_or(year),
            m: month(self.month),
            d: self.day,
        };
        if date.y < 0
            || date.y > 9999
            || !(1..=12).contains(&date.m)
            || date.d < 1
            || date.d > month_days(date.y, date.m)
        {
            return Err(VALUE_ERROR);
        }
        Ok(date)
    }
}

#[cfg(test)]
#[path = "time_grammar_tests.rs"]
mod tests;
