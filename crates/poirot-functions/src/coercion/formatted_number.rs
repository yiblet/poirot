//! Argument admission around the shared locale-parametric numeric grammar.
use crate::coercion::{CellError, compatibility::NumericTextDecision};
use poirot_parsing::number::NumberLocale;

/// Admit storage and work before trying the shared numeric grammar.
/// Failed alternatives retain their reservation; no-match permits calendar fallback.
pub fn coercev2_formatted_number<E>(
    text: &str,
    locale: &NumberLocale,
    mut admit: impl FnMut(usize, u64) -> Result<(), E>,
) -> Result<NumericTextDecision, E> {
    admit(text.len(), (text.len() as u64).saturating_mul(12))?;
    Ok(
        match poirot_parsing::number::parse_formatted_number(text, locale) {
            poirot_parsing::number::FormattedNumber::Number(value) => {
                NumericTextDecision::Number(value.map_err(|_| CellError::Value))
            }
            poirot_parsing::number::FormattedNumber::DateTime => NumericTextDecision::DateTime,
        },
    )
}
