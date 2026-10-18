//! Pinned formatter syntax with committed optional sections.
use crate::date::DateSyntaxError;
use nom::{Parser, bytes::complete::take_while_m_n};
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
// year/month/day/hour/clock-hour/minute/second/nanos/ampm
/// Parsed formatter fields accumulated across committed optional pattern sections.
#[derive(Clone, Copy, Default)]
struct FormatterFields {
    fields: [Option<i64>; 9],
}
fn set(plan: &mut FormatterFields, key: usize, value: i64) -> Result<(), ()> {
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
        // SAFETY: This branch checks an ASCII sign prefix, so byte one is a UTF-8 boundary inside the nonempty input.
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
fn token<'a>(
    input: &'a str,
    letter: u8,
    width: usize,
    plan: &mut FormatterFields,
) -> Result<&'a str, ()> {
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
    plan: &mut FormatterFields,
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

/// Inspect all concatenated source patterns without resolving calendar fields.
pub fn parse_formatter_fields(mut input: &str) -> Result<[Option<i64>; 9], DateSyntaxError> {
    let mut plan = FormatterFields::default();
    for pattern in PATTERNS {
        input = section(input, pattern.as_bytes(), 0, pattern.len(), &mut plan)
            .map_err(|_| DateSyntaxError)?;
    }
    if !input.is_empty() {
        return Err(DateSyntaxError);
    }
    Ok(plan.fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn optional_sections_restore_fields_and_input() {
        let mut plan = FormatterFields::default();
        let pattern = b"[H:m][H:m]";
        let tail = section("12:x13:30", pattern, 0, pattern.len(), &mut plan).unwrap();
        assert_eq!(tail, "12:x13:30");
        assert_eq!(plan.fields, [None; 9]);
        let pattern = b"[H:m][ H:m]";
        let tail = section("12:30 13:30", pattern, 0, pattern.len(), &mut plan).unwrap();
        assert_eq!(tail, " 13:30");
        assert_eq!(plan.fields[3], Some(12));
        assert_eq!(plan.fields[5], Some(30));
    }
    #[test]
    fn signed_width_and_overflow_leave_source_parser_tail() {
        assert_eq!(numeric("-1:", 1, false), Ok((":", -1)));
        for text in ["-0:", "+1:"] {
            assert!(numeric(text, 1, false).is_err());
        }
        assert!(numeric("+2000/", 4, true).is_err());
        assert!(numeric("10000/", 4, true).is_err());
        assert_eq!(numeric("+10000/", 4, true), Ok(("/", 10000)));
        assert!(numeric("1/", 2, false).is_err());
        assert_eq!(numeric("01/", 2, false), Ok(("/", 1)));
        assert_eq!(
            numeric("9223372036854775808:", 1, false),
            Ok(("8:", 922337203685477580))
        );
        assert_eq!(
            numeric("-9223372036854775809:", 1, false),
            Ok(("9:", -922337203685477580))
        );
    }
}
