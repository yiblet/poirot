//! Explicit NUMBERVALUE argument grammars, including pinned Java behavior.

/// Catchable spreadsheet failures from pure text kernels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Invalid text-function argument (`#VALUE!`).
    Value,
    /// Numeric text-function argument is outside its supported domain (`#NUM!`).
    Num,
}
/// Source or unsupported text contracts that remain operational failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostFault {
    /// Source parser requires a nonempty separator.
    EmptySeparator,
    /// Separator spelling forms an invalid source parsing pattern.
    InvalidSeparatorPattern,
    /// Supplementary separator character needs a verified UTF-16 admission contract.
    UnsupportedSeparatorUnit,
}
/// NUMBERVALUE failure preserving spreadsheet errors versus operational refusals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseFailure {
    /// Catchable numeric-text parsing error.
    Spreadsheet(Error),
    /// Pinned source fault retained for operational handling.
    Source(HostFault),
}

/// Parse numeric text using explicit decimal and grouping separators.
pub fn coercev2_number_value(text: &str, decimal: u16, grouping: u16) -> Result<f64, ParseFailure> {
    let value_error = || ParseFailure::Spreadsheet(Error::Value);
    // These characters have special or invalid meanings inside Java's [x] regex.
    // A lone caret leaves an unclosed negated character class in Java.
    if decimal == b'\\' as u16 || decimal == b'[' as u16 || decimal == b'^' as u16 {
        return Err(ParseFailure::Source(HostFault::InvalidSeparatorPattern));
    }
    let mut units: Vec<u16> = if text.is_empty() {
        vec![b'0' as u16]
    } else {
        text.encode_utf16().filter(|c| *c != b' ' as u16).collect()
    };
    let mut parts: Vec<Vec<u16>> = vec![Vec::new()];
    for unit in &units {
        if *unit == decimal {
            parts.push(Vec::new());
        } else {
            // SAFETY: parts starts with one element and is only appended during collection; subsequent accesses are guarded by its length being one or two.
            parts.last_mut().unwrap().push(*unit);
        }
    }
    // Java split with limit=0 discards trailing empty fields; empty input remains one.
    if !units.is_empty() {
        while parts.last().is_some_and(Vec::is_empty) {
            parts.pop();
        }
    }
    if parts.len() > 2 {
        return Err(value_error());
    }
    if parts.len() == 2 {
        // SAFETY: parts starts with one element and is only appended during collection; subsequent accesses are guarded by its length being one or two.
        if parts[1].contains(&grouping) {
            return Err(value_error());
        }
        // SAFETY: parts starts with one element and is only appended during collection; subsequent accesses are guarded by its length being one or two.
        units = parts[0]
            .iter()
            .copied()
            .filter(|c| *c != grouping)
            .collect();
        units.push(b'.' as u16);
        // SAFETY: parts starts with one element and is only appended during collection; subsequent accesses are guarded by its length being one or two.
        units.extend_from_slice(&parts[1]);
    } else if parts.len() == 1 {
        // SAFETY: parts starts with one element and is only appended during collection; subsequent accesses are guarded by its length being one or two.
        units = parts[0]
            .iter()
            .copied()
            .filter(|c| *c != grouping)
            .collect();
    }
    let mut percent: usize = 0;
    while units.last() == Some(&(b'%' as u16)) {
        units.pop();
        // SAFETY: Each iteration removes one unit, so the count cannot exceed
        // the original Vec length, which is representable as usize.
        percent += 1;
    }
    let text = String::from_utf16(&units).map_err(|_| value_error())?;
    let number = poirot_parsing::java_number::parse_java_number(&text).ok_or_else(value_error)?
        / i32::try_from(percent).map_or(f64::INFINITY, |count| 100_f64.powi(count));
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ParseFailure::Spreadsheet(Error::Num))
    }
}

/// Parse documented NUMBERVALUE text, separately from the pinned Java grammar.
/// Explicit separator arguments use their first BMP character; supplementary
/// separators remain operationally unsupported pending native evidence. Omitted ones
/// use the supplied numeric locale. The host admits text allocation and work
/// before calling this kernel. Normalization never grows beyond the input bytes.
pub fn coercev2_number_value_excel(
    text: &str,
    locale: &poirot_parsing::number::NumberLocaleV2,
    decimal_argument: Option<&str>,
    grouping_argument: Option<&str>,
) -> Result<f64, ParseFailure> {
    use nom::{Parser, combinator::all_consuming};
    let value_error = || ParseFailure::Spreadsheet(Error::Value);
    if decimal_argument
        .into_iter()
        .chain(grouping_argument)
        .filter_map(|s| s.chars().next())
        .any(|c| c.len_utf16() != 1)
    {
        return Err(ParseFailure::Source(HostFault::UnsupportedSeparatorUnit));
    }
    let explicit_decimal = decimal_argument
        .map(|s| {
            s.chars()
                .next()
                .map(|c| c.to_string())
                .ok_or_else(value_error)
        })
        .transpose()?;
    let explicit_grouping = grouping_argument
        .and_then(|s| s.chars().next())
        .map(|c| c.to_string());
    let decimal = explicit_decimal
        .as_deref()
        .unwrap_or(locale.decimal_separator());
    let grouping = match grouping_argument {
        Some(_) => explicit_grouping.as_deref(),
        None => locale.grouping_separator(),
    };
    if grouping == Some(decimal) {
        return Err(value_error());
    }
    if text.is_empty() {
        return Ok(0.);
    }
    let mut normalized = String::new();
    let mut remaining = text;
    let mut decimal_seen = false;
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix(decimal) {
            if decimal_seen {
                return Err(value_error());
            }
            normalized.push('.');
            decimal_seen = true;
            remaining = rest;
        } else if let Some(rest) = grouping.and_then(|group| remaining.strip_prefix(group)) {
            if decimal_seen {
                return Err(value_error());
            }
            remaining = rest;
        } else {
            let mut chars = remaining.chars();
            let c = chars.next().ok_or_else(value_error)?;
            remaining = chars.as_str();
            if c == '.' && decimal != "." {
                return Err(value_error());
            }
            if c != ' ' {
                normalized.push(c);
            }
        }
    }
    let mut percent_count: usize = 0;
    while normalized.ends_with('%') {
        normalized.pop();
        // SAFETY: Each iteration removes one ASCII byte, bounding the count
        // by the initial String length, which is representable as usize.
        percent_count += 1;
    }
    all_consuming(poirot_parsing::number::source_float)
        .parse(normalized.as_str())
        .map_err(|_| value_error())?;
    let mut number =
        poirot_parsing::number::parse_rust_float(&normalized).ok_or_else(value_error)?;
    if !number.is_finite() {
        return Err(value_error());
    }
    // A nonzero significand that already underflows is an invalid numeric input;
    // percentage operations are applied only after successful number admission.
    let nonzero = normalized
        .split(['e', 'E'])
        .next()
        .is_some_and(|mantissa| mantissa.bytes().any(|b| matches!(b, b'1'..=b'9')));
    if number == 0. && nonzero {
        return Err(value_error());
    }
    // Repeated division preserves valid tiny results even when 100^count would
    // overflow; it also matches repeated worksheet percent operations.
    for _ in 0..percent_count {
        number /= 100.;
    }
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_numbervalue_preserves_tiny_percent_results_and_locale_tokens() {
        let locale = poirot_parsing::number::NumberLocaleV2::EN_US;
        let parse = |s| coercev2_number_value_excel(s, &locale, None, None);
        assert_eq!(parse("1.23456789e308"), Ok(1.23456789e308));
        assert_eq!(parse("1.23456789e-310"), Ok(1.23456789e-310));
        let many_percent = format!("50{}", "%".repeat(160));
        let scaled = parse(&many_percent).unwrap();
        assert!(scaled > 0. && scaled < 1e-318);
        assert_eq!(
            parse("1e-324"),
            Err(ParseFailure::Spreadsheet(Error::Value))
        );
        assert_eq!(parse("1e309"), Err(ParseFailure::Spreadsheet(Error::Value)));
        assert_eq!(parse("1,,2.5"), Ok(12.5));
        assert_eq!(parse("1.2,3"), Err(ParseFailure::Spreadsheet(Error::Value)));
        let synthetic =
            poirot_parsing::number::NumberLocaleV2::new("::", Some("__"), None, "%", ":");
        // Locale validation deliberately forbids overlapping decimal/time tokens.
        assert!(synthetic.is_none());
        let synthetic =
            poirot_parsing::number::NumberLocaleV2::new("::", Some("__"), None, "%", "|").unwrap();
        assert_eq!(
            coercev2_number_value_excel("1__234::5", &synthetic, None, None),
            Ok(1234.5)
        );
        assert_eq!(
            coercev2_number_value_excel("1.5", &synthetic, None, None),
            Err(ParseFailure::Spreadsheet(Error::Value))
        );
    }
    #[test]
    fn separators_spaces_percent_and_trailing_split() {
        let parse = |s| coercev2_number_value(s, '.' as u16, ',' as u16);
        assert_eq!(parse(" 3,0 00.25%%"), Ok(0.300025));
        assert_eq!(parse("12.."), Ok(12.));
        assert_eq!(parse("1.2,3"), Err(ParseFailure::Spreadsheet(Error::Value)));
        assert_eq!(parse(""), Ok(0.));
        assert_eq!(parse("NaN"), Err(ParseFailure::Spreadsheet(Error::Num)));
        assert_eq!(
            parse("Infinity"),
            Err(ParseFailure::Spreadsheet(Error::Num))
        );
        assert_eq!(parse("inf"), Err(ParseFailure::Spreadsheet(Error::Value)));
        assert_eq!(parse("0x1p2"), Ok(4.));
        assert_eq!(parse("7D"), Ok(7.));
    }
}
