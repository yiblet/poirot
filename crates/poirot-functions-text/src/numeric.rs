//! Java double syntax and String.split behavior used by NUMBERVALUE.
use crate::{Error, HostFault};
/// NUMBERVALUE failure preserving spreadsheet errors versus pinned source faults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseFailure {
    /// Catchable numeric-text parsing error.
    Spreadsheet(Error),
    /// Pinned source fault retained for operational handling.
    Source(HostFault),
}

/// Parse numeric text using explicit decimal and grouping separators.
pub fn number_value(text: &str, decimal: u16, grouping: u16) -> Result<f64, ParseFailure> {
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
    let mut percent = 0;
    while units.last() == Some(&(b'%' as u16)) {
        units.pop();
        percent += 1;
    }
    let text = String::from_utf16(&units).map_err(|_| value_error())?;
    let number =
        poirot_numeric::parse_java_number(&text).ok_or_else(value_error)? / 100_f64.powi(percent);
    if number.is_finite() {
        Ok(number)
    } else {
        Err(ParseFailure::Spreadsheet(Error::Num))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separators_spaces_percent_and_trailing_split() {
        let parse = |s| number_value(s, '.' as u16, ',' as u16);
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
