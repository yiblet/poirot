//! Numeric coercion contract of the deferred host and its pure grammar decisions.
//! Context lookup and admission stay in CallHost.
#[cfg(test)]
#[cfg(test)]
use poirot_functions::coercion::compatibility::NumericTextDecision as CoercionDecision;

/// Numeric coercion contract used by the deferred host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::eval) enum NumericPolicy {
    /// Pinned Apache POI conversion behavior.
    PinnedPoi,
}

/// Classify numeric text without reading context or consuming host resources.
#[cfg(test)]
pub(super) fn decide(text: &str, policy: NumericPolicy) -> CoercionDecision {
    match policy {
        NumericPolicy::PinnedPoi => {
            poirot_functions::coercion::compatibility::coerce_numeric_text(text)
        }
    }
}
