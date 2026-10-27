//! Explicit NUMBERVALUE argument grammars, including pinned Java behavior.
pub use crate::coercion::{NumericTextError, NumericTextParseError, NumericTextSourceError};

/// Parse numeric text using explicit decimal and grouping separators.
pub fn coerce_number_value(
    text: &str,
    decimal: u16,
    grouping: u16,
) -> Result<f64, NumericTextParseError> {
    let value_error = || NumericTextParseError::Spreadsheet(NumericTextError::Value);
    // These characters have special or invalid meanings inside Java's [x] regex.
    // A lone caret leaves an unclosed negated character class in Java.
    if decimal == b'\\' as u16 || decimal == b'[' as u16 || decimal == b'^' as u16 {
        return Err(NumericTextParseError::Source(
            NumericTextSourceError::InvalidSeparatorPattern,
        ));
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
        Err(NumericTextParseError::Spreadsheet(NumericTextError::Num))
    }
}

/// Parse documented NUMBERVALUE text, separately from the pinned Java grammar.
/// Explicit separator arguments use their first BMP character; supplementary
/// separators remain operationally unsupported pending native evidence. Omitted ones
/// use the supplied numeric locale. The host admits text allocation and work
/// before calling this kernel. Normalization never grows beyond the input bytes.
pub fn coerce_number_value_excel(
    text: &str,
    locale: &poirot_parsing::number::NumberLocale,
    decimal_argument: Option<&str>,
    grouping_argument: Option<&str>,
) -> Result<f64, NumericTextParseError> {
    use nom::{Parser, combinator::all_consuming};
    let value_error = || NumericTextParseError::Spreadsheet(NumericTextError::Value);
    if decimal_argument
        .into_iter()
        .chain(grouping_argument)
        .filter_map(|s| s.chars().next())
        .any(|c| c.len_utf16() != 1)
    {
        return Err(NumericTextParseError::Source(
            NumericTextSourceError::UnsupportedSeparatorUnit,
        ));
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
    if text.bytes().all(|byte| byte == b' ') {
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
    // NUMBERVALUE admits the converted decimal after percentage scaling.
    // Text conversion uses the worksheet input domain, not formula-result binary64 bounds.
    // https://learn.microsoft.com/en-us/office/client-developer/excel/excel-worksheet-and-expression-evaluation
    if number.abs() > 9.99999999999999e307 || (number != 0. && number.abs() < f64::MIN_POSITIVE) {
        return Err(value_error());
    }
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_numbervalue_enforces_converted_input_range_and_locale_tokens() {
        let locale = poirot_parsing::number::NumberLocale::EN_US;
        let parse = |s| coerce_number_value_excel(s, &locale, None, None);
        assert_eq!(
            parse("1.23456789e308"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        assert_eq!(
            parse("1.23456789e-310"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        let many_percent = format!("50{}", "%".repeat(160));
        assert_eq!(
            parse(&many_percent),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        assert_eq!(
            parse("1e-324"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        assert_eq!(
            parse("1e309"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        assert_eq!(parse("1,,2.5"), Ok(12.5));
        assert_eq!(
            parse("1.2,3"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        let synthetic = poirot_parsing::number::NumberLocale::new("::", Some("__"), None, "%", ":");
        // Locale validation deliberately forbids overlapping decimal/time tokens.
        assert!(synthetic.is_err());
        let synthetic =
            poirot_parsing::number::NumberLocale::new("::", Some("__"), None, "%", "|").unwrap();
        assert_eq!(
            coerce_number_value_excel("1__234::5", &synthetic, None, None),
            Ok(1234.5)
        );
        assert_eq!(
            coerce_number_value_excel("1.5", &synthetic, None, None),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
    }
    #[test]
    fn separators_spaces_percent_and_trailing_split() {
        let parse = |s| coerce_number_value(s, '.' as u16, ',' as u16);
        assert_eq!(parse(" 3,0 00.25%%"), Ok(0.300025));
        assert_eq!(parse("12.."), Ok(12.));
        assert_eq!(
            parse("1.2,3"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        assert_eq!(parse(""), Ok(0.));
        assert_eq!(
            parse("NaN"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Num))
        );
        assert_eq!(
            parse("Infinity"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Num))
        );
        assert_eq!(
            parse("inf"),
            Err(NumericTextParseError::Spreadsheet(NumericTextError::Value))
        );
        assert_eq!(parse("0x1p2"), Ok(4.));
        assert_eq!(parse("7D"), Ok(7.));
    }
}

#[cfg(test)]
mod error_tests {
    use super::{
        NumericTextError, NumericTextParseError, NumericTextSourceError, coerce_number_value,
    };
    use std::error::Error;

    #[test]
    fn numeric_text_failures_retain_their_exact_category_and_cause() {
        let input_error = coerce_number_value("bad", '.' as u16, ',' as u16).unwrap_err();
        assert_eq!(
            input_error,
            NumericTextParseError::Spreadsheet(NumericTextError::Value)
        );
        assert_eq!(
            input_error
                .source()
                .and_then(|cause| cause.downcast_ref::<NumericTextError>()),
            Some(&NumericTextError::Value)
        );
        let source_error = coerce_number_value("12", '[' as u16, ',' as u16).unwrap_err();
        assert_eq!(
            source_error,
            NumericTextParseError::Source(NumericTextSourceError::InvalidSeparatorPattern)
        );
        assert_eq!(
            source_error
                .source()
                .and_then(|cause| cause.downcast_ref::<NumericTextSourceError>()),
            Some(&NumericTextSourceError::InvalidSeparatorPattern)
        );
        fn assert_send_sync_static<T: Send + Sync + 'static>() {}
        assert_send_sync_static::<NumericTextParseError>();
    }
}
