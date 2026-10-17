//! Numeric and text coercion contracts, including attributed context effects.
use super::*;

impl CallHost<'_, '_> {
    /// Resolve and numerically coerce one argument under explicit missing and numeric rules.
    pub(crate) fn coercev2_argument_number(
        &mut self,
        index: usize,
        missing: MissingPolicy,
        policy: NumericPolicy,
    ) -> FunctionResult<f64> {
        let value = self.scalar(index, missing)?;
        match value {
            Text(text) => self.coercev2_number_text(&text, policy),
            value => self.coercev2_scalar_number(&value, policy),
        }
    }
    /// Coerce a scalar using shared numeric conversion and attributed date fallback.
    pub(crate) fn coercev2_scalar_number(
        &mut self,
        value: &CellValue,
        policy: NumericPolicy,
    ) -> FunctionResult<f64> {
        if let Some(text) = value.text_ref() {
            let text = checked_text_view(text, &mut self.session.budget)?;
            self.coercev2_number_text(&text, policy)
        } else {
            coercev2_arithmetic_number(value, &mut self.session.budget)
        }
    }
    /// Render a scalar under shared text coercion and allocation limits.
    pub(crate) fn coercev2_scalar_text(&mut self, value: &CellValue) -> FunctionResult<String> {
        coercev2_formula_text(value, &mut self.session.budget)
    }
    /// Project model text into a checked owned helper string.
    pub(crate) fn project_text(
        &mut self,
        value: poirot_model::TextRef<'_>,
    ) -> FunctionResult<String> {
        checked_text_view(value, &mut self.session.budget).map(|text| text.into_owned())
    }
    /// Parse numeric text, recording context consumed by any date-time fallback.
    pub(crate) fn coercev2_number_text(
        &mut self,
        text: &str,
        policy: NumericPolicy,
    ) -> FunctionResult<f64> {
        self.check_text(text.len())?;
        let decision = match policy {
            #[cfg(test)]
            NumericPolicy::LegacyCompatible => numeric_policy::decide(text, policy),
            NumericPolicy::PinnedPoi => self.session.coercev2_formatted_text(text)?,
        };
        match decision {
            numeric_policy::CoercionV2Decision::Number(result) => result.map_err(Into::into),
            numeric_policy::CoercionV2Decision::DateTime => self
                .session
                .coercev2_numeric_date_time_at(text, Some((self.sheet, self.address))),
        }
    }
    /// Render a number with the selected source-compatible numeric formatter.
    pub(crate) fn coercev2_number_to_text(
        &mut self,
        value: f64,
        policy: NumericPolicy,
    ) -> FunctionResult<String> {
        let source = match policy {
            #[cfg(test)]
            NumericPolicy::LegacyCompatible => poirot_coercionv2::NumberTextSource::PoirotF6c572d,
            NumericPolicy::PinnedPoi => poirot_coercionv2::NumberTextSource::Poi7321209,
        };
        poirot_coercionv2::compatibility::coercev2_render_finite_number(value, source, || {
            self.check_text(poirot_numeric::number_text::ALLOCATION_BOUND)
        })
    }
    /// Compare scalars using pinned equality and preserve operational failures.
    pub(crate) fn equal(
        &mut self,
        left: &CellValue,
        right: &CellValue,
        policy: NumericPolicy,
    ) -> FunctionResult<bool> {
        if policy == NumericPolicy::PinnedPoi {
            for value in [left, right] {
                if let Error(error) = value {
                    return Err((*error).into());
                }
            }
            // TODO(coercion-non-argument): [LOW] Equality collapses numeric/blank pairs; reusing those values outside comparison would alter type-sensitive behavior.
            let numeric = coercev2_equality_numeric_pair(left, right);
            if let Some((left, right)) = numeric {
                self.charge_work(1)?;
                return Ok(poirot_numeric::compare(left, right)?.is_eq());
            }
            if [left, right].iter().any(|value| {
                value
                    .text_ref()
                    .is_some_and(|text| text.units().any(|unit| unit > 127))
            }) {
                return Err(self.unsupported(
                    CapabilityId::FunctionAdapter,
                    "pinned Unicode equality needs the Java-compatible comparison policy",
                ));
            }
        }
        for value in [left, right] {
            if let Text(text) = value {
                self.check_text(
                    text.chars()
                        .flat_map(char::to_lowercase)
                        .map(char::len_utf8)
                        .sum(),
                )?;
            }
        }
        self.charge_work(1)?;
        Ok(compare(left, right, &mut self.session.budget)?.is_eq())
    }
}

/// Pinned numeric equality admits blanks as zero; other type pairs remain typed.
fn coercev2_equality_numeric_pair(left: &CellValue, right: &CellValue) -> Option<(f64, f64)> {
    poirot_coercionv2::compatibility::coercev2_equality_numeric_pair(left, right)
}
