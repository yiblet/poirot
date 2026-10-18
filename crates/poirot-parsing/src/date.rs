//! Borrowed date fields. Calendar validation and default-year reads are consumer work.
use nom::{
    IResult, Parser,
    bytes::complete::take_while1,
    character::complete::char,
    combinator::{all_consuming, opt, rest},
    multi::separated_list1,
    sequence::{preceded, terminated},
};

/// Date text does not match the selected field grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateSyntaxError;

/// Parse DATEVALUE fields and its admitted trailing time text.
pub fn date_parts(input: &str) -> Result<Vec<&str>, DateSyntaxError> {
    fn word(s: &str) -> IResult<&str, &str> {
        take_while1(|c: char| c.is_ascii_alphanumeric())(s)
    }
    all_consuming(terminated(
        separated_list1(
            char(if input.split(' ').next().unwrap_or("").contains('/') {
                '/'
            } else {
                '-'
            }),
            word,
        ),
        opt(preceded(char(' '), rest)),
    ))
    .parse(input)
    .map(|(_, parts)| parts)
    .map_err(|_| DateSyntaxError)
}

use nom::bytes::complete::take_while_m_n;
/// Inspected TIMEVALUE date components before optional default-year resolution.
#[derive(Clone, Copy)]
pub struct TimeValueDate<'a> {
    pub year: Option<i64>,
    pub month: &'a str,
    pub day: i64,
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
// DateParser regex order: YMD dash, DMY dash, MD dash, MDY slash,
// YMD slash, MD slash. At most six bounded attempts, no field vector.
fn fields(input: &str, index: usize) -> IResult<&str, TimeValueDate<'_>> {
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
        TimeValueDate {
            year,
            month: mon,
            day: number(day),
        },
    ))
}
impl<'a> TimeValueDate<'a> {
    /// Parse TIMEVALUE's independent date grammar without consulting context.
    pub fn inspect(input: &'a str) -> Result<Self, DateSyntaxError> {
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
        Err(DateSyntaxError)
    }
}
