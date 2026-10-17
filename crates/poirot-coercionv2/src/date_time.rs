//! Numeric-text datetime continuations with explicit modern and pinned contracts.
use nom::{IResult, Parser, bytes::complete::take_while_m_n};
/// Locale capability available to the pinned numeric-text date fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalePolicy {
    /// Recognize the supported English date/time names.
    English,
    /// Preserve an unavailable locale instead of substituting one.
    Unsupported,
}
/// Timezone context consumed when parsing date-like numeric text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimezonePolicy {
    /// Use an explicit offset rather than the host timezone.
    FixedOffsetSeconds(i32),
    /// Report unsupported timezone-dependent parsing.
    Unsupported,
}
/// Caller-supplied year, locale, and timezone used by the numeric-text date fallback.
/// The default year replaces the Java formatter's class-initialization year.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    /// Year substituted when input omits a year.
    pub default_year: i32,
    /// Locale contract available to the fallback parser.
    pub locale: LocalePolicy,
    /// Timezone contract available to the fallback parser.
    pub timezone: TimezonePolicy,
}
/// Which context fields a parse attempt consumed, for dependency tracking.
/// A failed/unsupported attempt may still consume context before it stops.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// Whether the parse attempt consumed the fallback year.
    pub default_year: bool,
    /// Whether the parse attempt consumed locale-dependent behavior.
    pub locale: bool,
    /// Whether the parse attempt consumed timezone-dependent behavior.
    pub timezone: bool,
}
/// Successful date serial with consumed context, ordinary non-date text, or an
/// unsupported parsing capability. Rejection is not itself a spreadsheet error.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// Successful serial value together with consumed context dependencies.
    Valid {
        /// Parsed spreadsheet date/time serial.
        value: f64,
        /// Context fields consumed by this attempt.
        effects: Effects,
    },
    /// Ordinary text that does not match the supported date fallback.
    Rejected,
    /// Parsing needs a capability outside the supplied context.
    Unsupported(&'static str),
}
const PATTERNS: [&str; 4] = [
    "[d[.] [MMMM][MMM][ yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[[yyyy ]d-[MMMM][MMM][-yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[M/dd[/yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[[yyyy/]M/dd][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
];
/// Tentative calendar/time components rolled back together for optional parse sections.
#[derive(Clone, Default)]
struct Fields {
    year: Option<i32>,
    month: Option<i32>,
    day: Option<i32>,
    hour: Option<i32>,
    clock: Option<i32>,
    minute: Option<i32>,
    second: Option<i32>,
    millis: Option<i32>,
    ampm: Option<i32>,
    names: bool,
}
fn assign(slot: &mut Option<i32>, value: i32) -> bool {
    if slot.is_some_and(|old| old != value) {
        return false;
    }
    *slot = Some(value);
    true
}
fn number(input: &str, min: usize, max: usize) -> IResult<&str, i32> {
    let (rest, digits) = take_while_m_n(min, max, |c: char| c.is_ascii_digit()).parse(input)?;
    Ok((rest, digits.parse().unwrap_or(i32::MAX)))
}
// Optional sections roll back input and fields together. Sections concatenate;
// they are not alternatives, and successful prefixes are never backtracked.
fn section(
    pattern: &[u8],
    cursor: &mut usize,
    input: &mut &str,
    f: &mut Fields,
    effects: &mut Effects,
) -> bool {
    while *cursor < pattern.len() {
        // SAFETY: The loop condition checks *cursor < pattern.len() before this byte access.
        let c = pattern[*cursor];
        *cursor += 1;
        if c == b']' {
            return true;
        }
        if c == b'[' {
            let old_input = *input;
            let old_f = f.clone();
            let start = *cursor;
            if !section(pattern, cursor, input, f, effects) {
                *input = old_input;
                *f = old_f;
                *cursor = start;
                let mut depth = 1;
                while *cursor < pattern.len() && depth > 0 {
                    // SAFETY: The loop condition checks *cursor < pattern.len() before this byte access.
                    match pattern[*cursor] {
                        b'[' => depth += 1,
                        b']' => depth -= 1,
                        _ => (),
                    }
                    *cursor += 1;
                }
            }
            continue;
        }
        let mut width = 1;
        if c.is_ascii_alphabetic() {
            // SAFETY: The loop condition checks *cursor < pattern.len() before this byte access.
            while *cursor < pattern.len() && pattern[*cursor] == c {
                width += 1;
                *cursor += 1;
            }
        }
        if c == b'M' && width >= 3 {
            effects.locale = true;
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
            let found = MONTHS.iter().enumerate().find_map(|(i, name)| {
                let name = if width == 3 { name.get(..3)? } else { name };
                input.strip_prefix(name).map(|rest| (rest, i as i32 + 1))
            });
            let Some((rest, m)) = found else {
                return false;
            };
            if !assign(&mut f.month, m) {
                return false;
            }
            f.names = true;
            *input = rest;
        } else if c == b'a' {
            effects.locale = true;
            let (rest, a) = if let Some(rest) = input.strip_prefix("AM") {
                (rest, 0)
            } else if let Some(rest) = input.strip_prefix("PM") {
                (rest, 1)
            } else {
                return false;
            };
            if !assign(&mut f.ampm, a) {
                return false;
            }
            *input = rest;
        } else if matches!(c, b'y' | b'M' | b'd' | b'h' | b'H' | b'm' | b's' | b'S') {
            let (min, max) = if width > 1 { (width, width) } else { (1, 19) };
            let parsed = if c == b'y' {
                if let Some(unsigned) = input.strip_prefix('+') {
                    number(unsigned, 5, 19)
                } else {
                    // yyyy uses EXCEEDS_PAD: an unsigned run longer than four
                    // digits fails the entire optional, rather than splitting.
                    match number(input, 4, 19) {
                        Ok((rest, value)) if input.len() - rest.len() == 4 => Ok((rest, value)),
                        _ => return false,
                    }
                }
            } else {
                number(input, min, max)
            };
            let Ok((rest, n)) = parsed else {
                return false;
            };
            let slot = match c {
                b'y' => &mut f.year,
                b'M' => &mut f.month,
                b'd' => &mut f.day,
                b'h' => &mut f.clock,
                b'H' => &mut f.hour,
                b'm' => &mut f.minute,
                b's' => &mut f.second,
                _ => &mut f.millis,
            };
            if !assign(slot, n) {
                return false;
            }
            *input = rest;
        } else {
            let Some(rest) = input.strip_prefix(c as char) else {
                return false;
            };
            *input = rest;
        }
    }
    true
}
fn leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}
fn days(y: i32, m: i32, d: i32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yo = y - era * 400;
    let m = i64::from(m) + if m > 2 { -3 } else { 9 };
    era * 146097 + yo * 365 + yo / 4 - yo / 100 + (153 * m + 2) / 5 + i64::from(d) - 1
}
/// Call after the ordinary numeric parser rejects the text. `default_year` is
/// the frozen replacement for the source formatter's class-initialization year.
pub fn coercev2_date_time(text: &str, context: Context) -> Outcome {
    coercev2_date_time_with_effects(text, context).0
}
/// Retains context effects when parsing aborts on an unsupported capability.
/// This permits callers to trace failed context guards without inferring fields.
pub fn coercev2_date_time_with_effects(text: &str, context: Context) -> (Outcome, Effects) {
    let mut effects = Effects::default();
    let outcome = parse_inner(text, context, &mut effects, false);
    (outcome, effects)
}
fn parse_inner(
    text: &str,
    context: Context,
    effects: &mut Effects,
    calendar_clock: bool,
) -> Outcome {
    if context.locale != LocalePolicy::English {
        effects.locale = true;
        return Outcome::Unsupported("formatter locale");
    }
    let mut normalized = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') {
            if !normalized.ends_with(' ') {
                normalized.push(' ');
            }
        } else {
            normalized.push(c);
        }
    }
    let mut input = normalized.as_str();
    let mut f = Fields::default();
    for p in PATTERNS {
        if !section(p.as_bytes(), &mut 0, &mut input, &mut f, effects) {
            return Outcome::Rejected;
        }
    }
    if !input.is_empty()
        || (calendar_clock && (f.year.is_some() || f.month.is_some() || f.day.is_some()))
    {
        return Outcome::Rejected;
    }
    if f.year.is_some_and(|y| y < 1) || f.month.is_some_and(|m| !(1..=12).contains(&m)) {
        return Outcome::Rejected;
    }
    effects.locale |= f.names || f.ampm.is_some();
    let date = f.month.is_some() && f.day.is_some();
    let time = f.minute.is_some() && (f.hour.is_some() || f.clock.is_some());
    if !date && !time {
        return Outcome::Rejected;
    }
    let mut seconds = 0;
    let mut excess = 0;
    if time {
        let h = match (f.hour, f.clock, f.ampm) {
            (Some(h), None, _) => h,
            (_, Some(c), Some(a)) if (0..=12).contains(&c) => {
                let h = c % 12 + 12 * a;
                if f.hour.is_some_and(|other| other != h) {
                    return Outcome::Rejected;
                }
                h
            }
            _ => return Outcome::Rejected,
        };
        // SAFETY: The local date/time flags require these fields to be Some; they are not changed before this access.
        let m = f.minute.unwrap();
        let s = f.second.unwrap_or(0);
        if !(0..=59).contains(&m) || !(0..=59).contains(&s) || !(0..=24).contains(&h) {
            return Outcome::Rejected;
        }
        if h == 24 {
            if m != 0 || s != 0 || f.millis.unwrap_or(0) != 0 {
                return Outcome::Rejected;
            }
            excess = 1;
        } else {
            seconds = h * 3600 + m * 60 + s;
        }
    }
    let mut serial = 0.0;
    if date {
        effects.timezone = true;
        if !matches!(context.timezone,TimezonePolicy::FixedOffsetSeconds(offset) if (-64800..=64800).contains(&offset))
        {
            return Outcome::Unsupported("date timezone");
        }
        effects.default_year = f.year.is_none();
        let y = f.year.unwrap_or(context.default_year);
        if f.year.is_none() && !(1..=10000).contains(&y) {
            return Outcome::Unsupported("default year outside proven calendar range");
        }
        if !(1..=999_999_999).contains(&y) {
            return Outcome::Rejected;
        }
        if y > 10000 {
            return Outcome::Unsupported("extended year calendar conversion");
        }
        // SAFETY: The local date/time flags require these fields to be Some; they are not changed before this access.
        let m = f.month.unwrap();
        // SAFETY: The local date/time flags require these fields to be Some; they are not changed before this access.
        let d = f.day.unwrap();
        if !(1..=999_999_999).contains(&y) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return Outcome::Rejected;
        }
        let limit = match m {
            2 => {
                if leap(y) {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        let absolute = days(y, m, d.min(limit)) + excess;
        if absolute < days(1900, 1, 1) {
            serial = -1.0;
        } else {
            serial = (absolute - days(1899, 12, 31)) as f64;
            if absolute >= days(1900, 3, 1) {
                serial += 1.0;
            }
        }
    }
    Outcome::Valid {
        value: serial
            + f64::from(seconds) / 86400.0
            + if calendar_clock {
                excess as f64 + f64::from(f.millis.unwrap_or(0)) / 86400000.
            } else {
                0.
            },
        effects: *effects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context(year: i32) -> Context {
        Context {
            default_year: year,
            locale: LocalePolicy::English,
            timezone: TimezonePolicy::FixedOffsetSeconds(0),
        }
    }
    fn valid(text: &str, year: i32) -> (f64, Effects) {
        match coercev2_date_time(text, context(year)) {
            Outcome::Valid { value, effects } => (value, effects),
            other => panic!("{text:?}: {other:?}"),
        }
    }
    #[test]
    fn smart_dates_and_1900_convention() {
        assert_eq!(valid("29 Feb 2023", 2000).0, valid("28 Feb 2023", 2000).0);
        assert_eq!(valid("31 Feb 2024", 2000).0, valid("29 Feb 2024", 2000).0);
        assert_eq!(valid("31 Apr 2024", 2000).0, valid("30 Apr 2024", 2000).0);
        assert_eq!(valid("1 Jan 1900", 2000).0, 1.0);
        assert_eq!(valid("28 Feb 1900", 2000).0, 59.0);
        assert_eq!(valid("1 Mar 1900", 2000).0, 61.0);
        assert_eq!(valid("1 Jan +10000", 2000).0, 2958466.0);
        assert_eq!(valid("+10000/1/02", 2000).0, 2958467.0);
        assert_eq!(valid("1 Jan 1899 12:00", 2000).0, -0.5);
    }
    #[test]
    fn context_year_is_used_only_when_omitted() {
        assert_eq!(valid("1 Jan 2024", 0), valid("1 Jan 2024", 2025));
        assert!(!valid("1 Jan 2024", 0).1.default_year);
        assert!(valid("1 Jan", 2024).1.default_year);
        assert_eq!(valid("1 Jan", 2024).0, valid("1 Jan 2024", 0).0);
        assert_eq!(valid("12:30", 0).1, Effects::default());
    }
    #[test]
    fn optional_sections_are_concatenated_and_conflicts_reject() {
        assert_eq!(
            valid("1 JanuaryJan 2024", 2000).0,
            valid("1 Jan 2024", 2000).0
        );
        for text in [
            "1 Jan 20241-Jan-2024",
            "1/02/20242024/1/02",
            "1 JanJan 2024",
            "1 JanuaryFeb 2024",
            "1 JanFeb 2024",
            "1 Jan 20242-Jan-2024",
            "2023 1-Jan-2024",
        ] {
            assert_eq!(coercev2_date_time(text, context(2000)), Outcome::Rejected);
        }
    }
    #[test]
    fn times_fraction_truncation_and_excess_day() {
        assert_eq!(valid("12:30 PM", 2000).0, valid("12:30", 2000).0);
        assert_eq!(valid("0:30 AM", 2000).0, 1800.0 / 86400.0);
        assert_eq!(valid("0:30 PM", 2000).0, 45000.0 / 86400.0);
        assert_eq!(valid("32 12:00", 2000).0, 0.5);
        assert_eq!(valid("1 12:00", 2000).0, 0.5);
        assert_eq!(valid("12:30 AM", 2000).0, valid("0:30", 2000).0);
        assert_eq!(valid("1:02:03.999", 2000).0, 3723.0 / 86400.0);
        assert_eq!(valid("24:00", 2000).0, 0.0);
        assert_eq!(
            valid("1 Jan 2024 24:00", 2000).0,
            valid("2 Jan 2024", 2000).0
        );
    }
    #[test]
    fn casing_whitespace_and_ordinary_invalid_text() {
        assert_eq!(
            valid("1\t\r\nJan 2024", 2000).0,
            valid("1 Jan 2024", 2000).0
        );
        assert_eq!(valid("1/02/2024", 2000).0, valid("2024/1/02", 2000).0);
        for text in [
            "garbage",
            "250%",
            "$12",
            "1,234",
            "",
            " ",
            " 1 Jan 2024",
            "1 Jan 2024 ",
            "1 january 2024",
            "1\u{a0}Jan 2024",
            "12:30 pm",
            "1:02:03.12",
            "24:01",
            "13:30 PM",
            "32 Jan 2024",
            "1 Jan 0000",
            "1/2/2024",
        ] {
            assert_eq!(
                coercev2_date_time(text, context(2000)),
                Outcome::Rejected,
                "{text:?}"
            );
        }
    }
    #[test]
    fn unsupported_policies_are_distinct() {
        let (outcome, effects) = coercev2_date_time_with_effects("1 nope", context(2024));
        assert_eq!(outcome, Outcome::Rejected);
        assert_eq!(
            effects,
            Effects {
                locale: true,
                ..Effects::default()
            }
        );
        let (outcome, effects) = coercev2_date_time_with_effects("1 Jan", context(0));
        assert!(matches!(outcome, Outcome::Unsupported(_)));
        assert_eq!(
            effects,
            Effects {
                default_year: true,
                locale: true,
                timezone: true
            }
        );
        let mut c = context(2024);
        c.locale = LocalePolicy::Unsupported;
        assert!(matches!(
            coercev2_date_time("1 Jan 2024", c),
            Outcome::Unsupported(_)
        ));
        c = context(2024);
        c.timezone = TimezonePolicy::Unsupported;
        assert!(matches!(
            coercev2_date_time("1 Jan 2024", c),
            Outcome::Unsupported(_)
        ));
        assert!(matches!(
            coercev2_date_time("12:00", c),
            Outcome::Valid { .. }
        ));
    }
}

/// Calendar continuation for modern numeric arguments. The calendar plan owns
/// variable-width civil dates and date ordering; the pinned formatter remains
/// available for its distinct named-date and standalone-clock grammar.
pub enum CalendarTextPlan<'a> {
    /// Civil date with an optional, fully validated clock fraction.
    Civil {
        /// Calendar fields awaiting explicit year and date-order context.
        date: crate::dates::text::DateTextPlan<'a>,
        /// Fraction of a day from an optional standalone clock.
        fraction: f64,
    },
    /// Text outside the modern civil-date grammar retains the pinned formatter.
    Formatter,
}
/// Inspect modern calendar text without reading workbook or clock context.
pub fn coercev2_calendar_text_plan(input: &str) -> Result<CalendarTextPlan<'_>, crate::CellError> {
    let input = input.trim();
    let (date, clock) = input.split_once(char::is_whitespace).unwrap_or((input, ""));
    let Ok(date) = crate::dates::text::DateTextPlan::inspect(date) else {
        return Ok(CalendarTextPlan::Formatter);
    };
    let fraction = if clock.is_empty() {
        0.
    } else {
        match parse_inner(
            clock.trim(),
            Context {
                default_year: 0,
                locale: LocalePolicy::English,
                timezone: TimezonePolicy::FixedOffsetSeconds(0),
            },
            &mut Effects::default(),
            true,
        ) {
            Outcome::Valid { value, .. } => value,
            _ => return Err(crate::CellError::Value),
        }
    };
    Ok(CalendarTextPlan::Civil { date, fraction })
}

/// Explicit workbook epoch for modern calendar-text argument admission.
#[derive(Clone, Copy, Debug)]
pub enum CalendarEpoch {
    /// The 1900 serial calendar includes Excel's fictitious leap-day slot.
    Excel1900,
    /// The 1904 calendar begins on Gregorian January 1, 1904.
    Excel1904,
}
/// Convert a validated civil argument to its workbook serial. Civil text does
/// not invent February 29, 1900; serial values retain that separate Excel slot.
pub fn coercev2_calendar_serial(
    date: crate::dates::calendar::Date,
    system: CalendarEpoch,
) -> Result<f64, crate::CellError> {
    use crate::dates::calendar::{Date, epoch, month_days};
    if !(1..=9999).contains(&date.y)
        || !(1..=12).contains(&date.m)
        || !(1..=month_days(date.y, date.m)).contains(&date.d)
    {
        return Err(crate::CellError::Value);
    }
    let day = epoch(date);
    let value = match system {
        CalendarEpoch::Excel1900 => {
            day - epoch(Date {
                y: 1899,
                m: 12,
                d: 31,
            }) + i64::from(
                day >= epoch(Date {
                    y: 1900,
                    m: 3,
                    d: 1,
                }),
            )
        }
        CalendarEpoch::Excel1904 => {
            day - epoch(Date {
                y: 1904,
                m: 1,
                d: 1,
            })
        }
    };
    Ok(value as f64)
}

#[cfg(test)]
mod calendar_tests {
    use super::*;
    use crate::dates::calendar::Date;
    #[test]
    fn civil_argument_epochs_preserve_early_serial_boundaries() {
        for (date, expected) in [
            (
                Date {
                    y: 1899,
                    m: 12,
                    d: 31,
                },
                0.,
            ),
            (
                Date {
                    y: 1900,
                    m: 1,
                    d: 1,
                },
                1.,
            ),
            (
                Date {
                    y: 1900,
                    m: 2,
                    d: 28,
                },
                59.,
            ),
            (
                Date {
                    y: 1900,
                    m: 3,
                    d: 1,
                },
                61.,
            ),
            (
                Date {
                    y: 1904,
                    m: 1,
                    d: 1,
                },
                1462.,
            ),
            (
                Date {
                    y: 9999,
                    m: 12,
                    d: 31,
                },
                2958465.,
            ),
        ] {
            assert_eq!(
                coercev2_calendar_serial(date, CalendarEpoch::Excel1900),
                Ok(expected)
            );
            // The phantom 1900 slot changes the difference before March 1900.
            let offset = if expected < 60. { 1461. } else { 1462. };
            assert_eq!(
                coercev2_calendar_serial(date, CalendarEpoch::Excel1904),
                Ok(expected - offset)
            );
        }
        for date in [
            Date {
                y: 1900,
                m: 2,
                d: 29,
            },
            Date {
                y: i64::MAX,
                m: 1,
                d: 1,
            },
            Date {
                y: 2020,
                m: 2,
                d: 30,
            },
        ] {
            assert_eq!(
                coercev2_calendar_serial(date, CalendarEpoch::Excel1900),
                Err(crate::CellError::Value)
            );
        }
    }
    #[test]
    fn modern_clock_suffix_is_complete_and_rolls_midnight_forward() {
        for (input, expected) in [
            ("1/2/2020 12:00", 0.5),
            ("1/2/2020 24:00", 1.),
            ("1/2/2020 12:00:00.500", 0.5 + 0.5 / 86400.),
        ] {
            let CalendarTextPlan::Civil { fraction, .. } =
                coercev2_calendar_text_plan(input).unwrap()
            else {
                panic!("calendar expected")
            };
            assert_eq!(fraction, expected);
        }
        for input in ["1/2/2020 garbage", "1/2/2020 32 12:00", "1/2/2020 24:01"] {
            assert!(coercev2_calendar_text_plan(input).is_err(), "{input}");
        }
    }
}
