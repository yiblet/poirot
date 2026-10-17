//! Compares lookup keys with candidates under the selected text policy.
//! Type mismatches stay distinct from ordering; coercion and comparison work remain metered.
use super::*;

/// Comparison is key-to-candidate, with None representing POI TYPE_MISMATCH.
pub(super) fn lookup_compare(
    key: &CellValue,
    value: &CellValue,
    mode: SearchMode,
    is_match: bool,
    source: TextSourceVersion,
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
            let wildcard_pattern = if source == TextSourceVersion::Poi7321209OpenJdk21Unicode15 {
                budget.text(pattern.storage_bytes())?;
                budget.charge(WorkKind::Comparison, pattern.storage_bytes() as u64)?;
                if pattern.units().any(|u| u == 0x2a || u == 0x3f) {
                    Some(checked_text_view(pattern, budget)?)
                } else {
                    None
                }
            } else {
                let pattern = checked_text_view(pattern, budget)?;
                pattern.contains(['*', '?']).then_some(pattern)
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

    if source == TextSourceVersion::Poi7321209OpenJdk21Unicode15
        && let (Some(left), Some(right)) = (key.text_ref(), value.text_ref())
    {
        budget.text(left.storage_bytes())?;
        budget.text(right.storage_bytes())?;
        let work = poirot_functions_text::java_compare::work_bound(
            left.storage_bytes(),
            right.storage_bytes(),
        )
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| comparison_failure("Java comparison work size overflow"))?;
        budget.charge(WorkKind::Comparison, work)?;
        return poirot_functions_text::java_compare::compare(left.units(), right.units())
            .map(Some)
            .map_err(|_| comparison_failure("invalid pinned Java simple mapping asset"));
    }
    compare(key, value, budget).map(Some)
}

fn comparison_failure(detail: &'static str) -> FunctionFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        source: None,
        detail: detail.into(),
    }
    .into()
}

fn coercev2_classic_lookup_blank_key<'a>(key: &'a CellValue, zero: &'a CellValue) -> &'a CellValue {
    poirot_coercionv2::compatibility::coercev2_classic_lookup_blank_key(key, zero)
}

#[cfg(test)]
mod java_tests {
    use super::*;
    #[test]
    fn source_derived_textref_representations_and_policy() {
        let mut budget = BudgetMeter::new(EvaluationBudgets::default()).unwrap();
        let pinned = TextSourceVersion::Poi7321209OpenJdk21Unicode15;
        let left = CellValue::from(poirot_model::ExcelText::from_units(vec![0x130]));
        let right = Text("i".into());
        assert_eq!(
            lookup_compare(
                &left,
                &right,
                SearchMode::Ascending,
                false,
                pinned,
                &mut budget
            ),
            Ok(Some(Ordering::Equal))
        );
        assert_ne!(
            lookup_compare(
                &left,
                &right,
                SearchMode::Ascending,
                false,
                TextSourceVersion::PoirotF6c572d,
                &mut budget
            ),
            Ok(Some(Ordering::Equal))
        );
        let lone = CellValue::from(poirot_model::ExcelText::from_units(vec![0xd800]));
        assert_eq!(
            lookup_compare(&lone, &lone, SearchMode::Exact, true, pinned, &mut budget),
            Ok(Some(Ordering::Equal))
        );
    }
    #[test]
    fn comparison_work_admission_is_operational() {
        let limits = EvaluationBudgets {
            run_work: 2,
            ..Default::default()
        };
        let mut budget = BudgetMeter::new(limits).unwrap();
        assert!(matches!(
            lookup_compare(
                &Text("I".into()),
                &Text("i".into()),
                SearchMode::Ascending,
                false,
                TextSourceVersion::Poi7321209OpenJdk21Unicode15,
                &mut budget
            ),
            Err(FunctionFailure::Operational(EvaluationFailure::Budget(_)))
        ));
    }
}
