//! Compares lookup keys with candidates under the selected text policy.
//! Type mismatches stay distinct from ordering; coercion and comparison work remain metered.
use super::*;

/// Comparison is key-to-candidate, with None representing POI TYPE_MISMATCH.
pub(super) fn lookup_compare(
    key: &CellValue,
    value: &CellValue,
    mode: SearchMode,
    is_match: bool,
    budget: &mut BudgetMeter,
) -> FunctionResult<Option<Ordering>> {
    budget.charge(WorkKind::Comparison, 1)?;
    let zero = Number(0.);
    // TODO(coercion-non-argument): [MEDIUM] Blank search keys become numeric zero; applying this to candidates or selected results would change blank lookup behavior.
    let key = coercev2_classic_lookup_blank_key(key, &zero);
    let same = (key.text_ref().is_some() && value.text_ref().is_some())
        || matches!(
            (key, value),
            (Number(_), Number(_)) | (Boolean(_), Boolean(_))
        );
    if !same {
        return Ok(None);
    }
    if let (Some(pattern), Some(candidate)) = (key.text_ref(), value.text_ref()) {
        // Preserve the legacy scalar projection and its admission effects.
        if is_match || mode == SearchMode::Exact {
            let wildcard_pattern = if pattern.units().any(|u| matches!(u, 0x2a | 0x3f)) {
                Some(checked_text_view(pattern, budget)?)
            } else {
                None
            };
            if let Some(pattern) = wildcard_pattern {
                let candidate = checked_text_view(candidate, budget)?;
                return Ok(Some(if wildcard(&pattern, &candidate, budget)? {
                    Ordering::Equal
                } else {
                    Ordering::Less
                }));
            }
        }
    }

    compare(key, value, budget).map(Some)
}

fn coercev2_classic_lookup_blank_key<'a>(key: &'a CellValue, zero: &'a CellValue) -> &'a CellValue {
    poirot_functions::coercion::compatibility::coercev2_classic_lookup_blank_key(key, zero)
}
