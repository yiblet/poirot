//! Source-specific optional datetime syntax; calendar resolution is consumer work.
use crate::date::DateSyntaxError;
use nom::{IResult, Parser, bytes::complete::take_while_m_n};
const PATTERNS: [&str; 4] = [
    "[d[.] [MMMM][MMM][ yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[[yyyy ]d-[MMMM][MMM][-yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[M/dd[/yyyy]][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
    "[[yyyy/]M/dd][[ ]h:m[:s][.SSS] a][[ ]H:m[:s][.SSS]]",
];
/// Tentative calendar/time components rolled back together for optional parse sections.
#[derive(Clone, Default)]
pub struct SourceDateTimeFields {
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub day: Option<i32>,
    pub hour: Option<i32>,
    pub clock: Option<i32>,
    pub minute: Option<i32>,
    pub second: Option<i32>,
    pub millis: Option<i32>,
    pub ampm: Option<i32>,
    pub names: bool,
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
    f: &mut SourceDateTimeFields,
    locale_read: &mut bool,
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
            if !section(pattern, cursor, input, f, locale_read) {
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
            *locale_read = true;
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
            *locale_read = true;
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

/// Collect source fields, preserving locale-token observations even on rejection.
/// The consumer admits storage before this normalization and grammar pass.
pub fn parse_source_fields(
    text: &str,
    locale_read: &mut bool,
) -> Result<SourceDateTimeFields, DateSyntaxError> {
    let mut normalized = String::new();
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
    let mut f = SourceDateTimeFields::default();
    for p in PATTERNS {
        if !section(p.as_bytes(), &mut 0, &mut input, &mut f, locale_read) {
            return Err(DateSyntaxError);
        }
    }
    if !input.is_empty() {
        return Err(DateSyntaxError);
    }
    Ok(f)
}
