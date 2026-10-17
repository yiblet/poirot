use crate::{CoercionV2Context, CoercionV2Failure, TextRef};
use std::borrow::Cow;

/// Checked representation projection, without substituting malformed text.
pub(crate) fn coercev2_text_view<'a>(
    text: TextRef<'a>,
    context: &mut CoercionV2Context,
) -> Result<Cow<'a, str>, CoercionV2Failure> {
    let work = u64::try_from(text.storage_bytes())
        .map_err(|_| CoercionV2Failure::Limit(crate::Resource::Work))?;
    context.charge(work)?;
    match text {
        TextRef::Utf8(text) => Ok(Cow::Borrowed(text)),
        TextRef::Utf16(units) => {
            let bytes = text
                .utf8_len()
                .map_err(|_| CoercionV2Failure::UnsupportedText)?;
            context.admit_text(bytes)?;
            String::from_utf16(units)
                .map(Cow::Owned)
                .map_err(|_| CoercionV2Failure::UnsupportedText)
        }
    }
}

/// Deliberately narrow, locale-independent finite decimal grammar.
/// No date, percent, fraction, grouping, NaN, or infinity spellings are admitted.
pub(crate) fn coercev2_decimal_text(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let mut bytes = text.bytes().peekable();
    if bytes.peek().is_some_and(|byte| matches!(byte, b'+' | b'-')) {
        bytes.next();
    }
    let mut has_digits = false;
    while bytes.peek().is_some_and(u8::is_ascii_digit) {
        bytes.next();
        has_digits = true;
    }
    if bytes.peek() == Some(&b'.') {
        bytes.next();
        while bytes.peek().is_some_and(u8::is_ascii_digit) {
            bytes.next();
            has_digits = true;
        }
    }
    if !has_digits {
        return None;
    }
    if bytes.peek().is_some_and(|byte| matches!(byte, b'e' | b'E')) {
        bytes.next();
        if bytes.peek().is_some_and(|byte| matches!(byte, b'+' | b'-')) {
            bytes.next();
        }
        let mut has_exponent_digits = false;
        while bytes.peek().is_some_and(u8::is_ascii_digit) {
            bytes.next();
            has_exponent_digits = true;
        }
        if !has_exponent_digits {
            return None;
        }
    }
    if bytes.next().is_some() {
        return None;
    }
    text.parse::<f64>().ok().filter(|number| number.is_finite())
}
