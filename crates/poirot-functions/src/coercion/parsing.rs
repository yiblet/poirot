use crate::coercion::{CoercionContext, CoercionFailure, TextRef};
use std::borrow::Cow;

/// Checked representation projection, without substituting malformed text.
pub(crate) fn coerce_text_view<'a>(
    text: TextRef<'a>,
    context: &mut CoercionContext,
) -> Result<Cow<'a, str>, CoercionFailure> {
    let work = u64::try_from(text.storage_bytes())
        .map_err(|_| CoercionFailure::Limit(crate::coercion::Resource::Work))?;
    context.charge(work)?;
    match text {
        TextRef::Utf8(text) => Ok(Cow::Borrowed(text)),
        TextRef::Utf16(units) => {
            let bytes = text
                .utf8_len()
                .map_err(|_| CoercionFailure::UnsupportedText)?;
            context.admit_text(bytes)?;
            String::from_utf16(units)
                .map(Cow::Owned)
                .map_err(|_| CoercionFailure::UnsupportedText)
        }
    }
}

/// Deliberately narrow, locale-independent finite decimal grammar.
/// No date, percent, fraction, grouping, NaN, or infinity spellings are admitted.
pub(crate) fn coerce_decimal_text(text: &str) -> Option<f64> {
    poirot_parsing::number::parse_finite_decimal(text)
}
