//! Pinned formatter syntax with committed optional sections.
use crate::date::DateSyntaxError;
use crate::formatter_error::FormatterError;
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
fn set(plan: &mut FormatterFields, key: usize, value: i64) -> Result<(), FormatterError> {
    let field = plan
        .fields
        .get_mut(key)
        .ok_or(FormatterError::InvalidField { index: key })?;
    if field.is_some_and(|old| old != value) {
        return Err(FormatterError::ConflictingField { index: key });
    }
    *field = Some(value);
    Ok(())
}
fn numeric(input: &str, width: usize, year: bool) -> Result<(&str, i64), FormatterError> {
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
    let (mut tail, digits) =
        result.map_err(|_| FormatterError::MissingDigits { minimum: width })?;
    if year && ((sign == "+" && digits.len() <= 4) || (sign.is_empty() && digits.len() > 4)) {
        return Err(FormatterError::InvalidYearSign);
    }
    let mut value = digits.parse::<i128>().map_err(FormatterError::Integer)?;
    if sign == "-" {
        if value == 0 {
            return Err(FormatterError::NegativeZero);
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
) -> Result<&'a str, FormatterError> {
    if letter == b'M' && width >= 3 {
        for (month, name) in MONTHS.iter().enumerate() {
            let name = if width == 3 {
                name.get(..3).ok_or(FormatterError::InvalidMonthName)?
            } else {
                name
            };
            if let Some(tail) = input.strip_prefix(name) {
                set(plan, 1, month as i64 + 1)?;
                return Ok(tail);
            }
        }
        return Err(FormatterError::MonthMismatch);
    }
    if letter == b'a' {
        for (name, value) in [("AM", 0), ("PM", 1)] {
            if let Some(tail) = input.strip_prefix(name) {
                set(plan, 8, value)?;
                return Ok(tail);
            }
        }
        return Err(FormatterError::MeridiemMismatch);
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
        _ => return Err(FormatterError::UnsupportedToken { token: letter }),
    };
    let (tail, mut value) = numeric(input, width, letter == b'y')?;
    if letter == b'S' {
        value *= 1_000_000;
    }
    set(plan, key, value)?;
    Ok(tail)
}
fn close(pattern: &[u8], mut index: usize) -> Result<usize, FormatterError> {
    let mut depth = 1;
    while depth != 0 {
        match *pattern
            .get(index)
            .ok_or(FormatterError::UnclosedSection { index })?
        {
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        };
        index += 1;
    }
    index
        .checked_sub(1)
        .ok_or(FormatterError::UnclosedSection { index })
}
fn section<'a>(
    mut input: &'a str,
    pattern: &[u8],
    mut index: usize,
    end: usize,
    plan: &mut FormatterFields,
) -> Result<&'a str, FormatterError> {
    if index > end || end > pattern.len() {
        return Err(FormatterError::InvalidSection {
            start: index,
            end,
            length: pattern.len(),
        });
    }
    while index < end {
        // SAFETY: The entry guard checks end <= pattern.len(); the outer and run-scanning loops check their cursors against end.
        let byte = pattern[index];
        if byte == b'[' {
            let finish = close(pattern, index + 1)?;
            let mut tentative = *plan;
            match section(input, pattern, index + 1, finish, &mut tentative) {
                Ok(tail) => {
                    input = tail;
                    *plan = tentative;
                }
                // Optional input mismatches restore both the input and accumulated fields.
                Err(
                    FormatterError::ConflictingField { .. }
                    | FormatterError::MissingDigits { .. }
                    | FormatterError::InvalidYearSign
                    | FormatterError::NegativeZero
                    | FormatterError::MonthMismatch
                    | FormatterError::MeridiemMismatch
                    | FormatterError::LiteralMismatch { .. },
                ) => {}
                Err(
                    error @ (FormatterError::InvalidField { .. }
                    | FormatterError::Integer(_)
                    | FormatterError::InvalidMonthName
                    | FormatterError::UnsupportedToken { .. }
                    | FormatterError::UnclosedSection { .. }
                    | FormatterError::InvalidSection { .. }),
                ) => return Err(error),
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
            input = input
                .strip_prefix(byte as char)
                .ok_or(FormatterError::LiteralMismatch { expected: byte })?;
            index += 1;
        }
    }
    Ok(input)
}

/// Inspect the pinned source patterns without resolving calendar fields.
///
/// The nine slots are, in order: year, one-based month, day of month, `H` hour,
/// `h` clock-hour, minute, second, fractional second in nanoseconds, and meridiem
/// (zero for `AM`, one for `PM`). Missing fields remain `None`; numeric ranges,
/// calendar validity, defaults and a serial-date epoch are not resolved here.
/// English month names and meridiem tokens are case-sensitive. Whitespace is
/// consumed literally rather than normalized. Optional sections restore input
/// and fields on failure; successful sections remain committed. Conflicting
/// repeated fields reject their section; unconsumed input produces
/// [`DateSyntaxError`].
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
    use super::{FormatterFields, numeric, section, set, token};
    use crate::formatter_error::FormatterError;
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
    fn helper_failures_retain_typed_context() {
        let mut plan = FormatterFields::default();
        assert_eq!(
            set(&mut plan, 9, 1),
            Err(FormatterError::InvalidField { index: 9 })
        );
        set(&mut plan, 3, 12).unwrap();
        assert_eq!(
            set(&mut plan, 3, 13),
            Err(FormatterError::ConflictingField { index: 3 })
        );
        assert_eq!(
            numeric("1/", 2, false),
            Err(FormatterError::MissingDigits { minimum: 2 })
        );
        assert_eq!(numeric("-0", 1, false), Err(FormatterError::NegativeZero));
        assert_eq!(
            numeric("+2000", 4, true),
            Err(FormatterError::InvalidYearSign)
        );
        assert_eq!(
            section("", b"[x]", 0, 3, &mut plan),
            Err(FormatterError::UnsupportedToken { token: b'x' })
        );
        assert_eq!(
            section("", b"[H", 0, 2, &mut plan),
            Err(FormatterError::UnclosedSection { index: 2 })
        );
        assert_eq!(
            section("", b"", 1, 0, &mut plan),
            Err(FormatterError::InvalidSection {
                start: 1,
                end: 0,
                length: 0
            })
        );
    }

    #[test]
    fn input_mismatches_keep_specific_categories() {
        let mut plan = FormatterFields::default();
        assert_eq!(
            token("Jan", b'M', 4, &mut plan),
            Err(FormatterError::MonthMismatch)
        );
        assert_eq!(
            token("am", b'a', 1, &mut plan),
            Err(FormatterError::MeridiemMismatch)
        );
        assert_eq!(
            section("-", b"/", 0, 1, &mut plan),
            Err(FormatterError::LiteralMismatch { expected: b'/' })
        );
    }

    #[test]
    fn numeric_conversion_preserves_its_typed_cause() {
        let source = "invalid".parse::<i128>().unwrap_err();
        let error = FormatterError::Integer(source.clone());
        assert_eq!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<std::num::ParseIntError>(),
            Some(&source)
        );
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
