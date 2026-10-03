//! Pure numeric grammar decisions. Context lookup and admission stay in CallHost.
use super::{CellError, NumericPolicy, VALUE_ERROR};
use nom::Parser;

/// Pure grammar result deciding whether numeric date fallback is required.
pub(super) enum CoercionDecision {
    /// Numeric grammar accepted the text, with its finite parse result.
    Number(Result<f64, CellError>),
    /// Text requires context-aware date-time coercion.
    DateTime,
}
/// Classify numeric text without reading context or consuming host resources.
pub(super) fn decide(text: &str, policy: NumericPolicy) -> CoercionDecision {
    match policy {
        #[cfg(test)]
        NumericPolicy::LegacyCompatible => CoercionDecision::Number(
            text.trim()
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .ok_or(VALUE_ERROR),
        ),
        NumericPolicy::PinnedPoi => {
            let trimmed = text.trim_matches(|c: char| (c as u32) <= 32);
            if nom::combinator::all_consuming(
                nom::number::complete::recognize_float::<_, nom::error::Error<_>>,
            )
            .parse(trimmed)
            .is_ok()
            {
                CoercionDecision::Number(
                    poirot_numeric::parse_java_number(trimmed)
                        .filter(|number| number.is_finite())
                        .ok_or(VALUE_ERROR),
                )
            } else {
                CoercionDecision::DateTime
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_control_trim_and_finite_grammar_keep_date_fallback_distinct() {
        assert!(matches!(decide("\u{1f}1.5\u{0}", NumericPolicy::PinnedPoi),
            CoercionDecision::Number(Ok(value)) if value == 1.5));
        assert!(matches!(
            decide("1e999", NumericPolicy::PinnedPoi),
            CoercionDecision::Number(Err(VALUE_ERROR))
        ));
        assert!(matches!(
            decide("\u{a0}1\u{a0}", NumericPolicy::PinnedPoi),
            CoercionDecision::DateTime
        ));
        assert!(matches!(
            decide("2020/01/02", NumericPolicy::PinnedPoi),
            CoercionDecision::DateTime
        ));
    }
}
