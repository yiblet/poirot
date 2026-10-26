//! Pure numeric grammar decisions. Context lookup and admission stay in CallHost.
#[cfg(test)]
use super::NumericPolicy;
#[cfg(test)]
use super::VALUE_ERROR;

/// Pure grammar result deciding whether numeric date fallback is required.
pub(super) use poirot_functions::coercion::compatibility::NumericTextDecision as CoercionV2Decision;
/// Classify numeric text without reading context or consuming host resources.
#[cfg(test)]
pub(super) fn decide(text: &str, policy: NumericPolicy) -> CoercionV2Decision {
    match policy {
        NumericPolicy::PinnedPoi => {
            poirot_functions::coercion::compatibility::coercev2_numeric_text(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_control_trim_and_finite_grammar_keep_date_fallback_distinct() {
        assert!(matches!(decide("\u{1f}1.5\u{0}", NumericPolicy::PinnedPoi),
            CoercionV2Decision::Number(Ok(value)) if value == 1.5));
        assert!(matches!(
            decide("1e999", NumericPolicy::PinnedPoi),
            CoercionV2Decision::Number(Err(VALUE_ERROR))
        ));
        assert!(matches!(
            decide("\u{a0}1\u{a0}", NumericPolicy::PinnedPoi),
            CoercionV2Decision::DateTime
        ));
        assert!(matches!(
            decide("2020/01/02", NumericPolicy::PinnedPoi),
            CoercionV2Decision::DateTime
        ));
    }
}
