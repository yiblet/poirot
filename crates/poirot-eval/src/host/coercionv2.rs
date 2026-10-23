//! Numeric and text coercion contracts, including attributed context effects.
use super::*;

impl CallHost<'_, '_> {
    /// Coerce a calendar serial under the existing date-family rules.
    pub(crate) fn coercev2_calendar_date(&mut self, value: &CellValue) -> FunctionResult<f64> {
        self.session.coercev2_date_argument(value)
    }

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
    /// Render a nonmissing scalar losslessly for UTF-16 text consumers.
    pub(crate) fn coercev2_text_units(&mut self, value: &CellValue) -> FunctionResult<Vec<u16>> {
        if *value == Blank {
            return Ok(Vec::new());
        }
        if let Some(text) = value.text_ref() {
            self.check_text_capacity(
                text.len_units()
                    .checked_mul(2)
                    .ok_or_else(|| self.budget_exceeded())?,
            )?;
            self.check_values_capacity(text.len_units())?;
            self.charge_work(text.len_units() as u64)?;
            let mut units = Vec::new();
            units
                .try_reserve_exact(text.len_units())
                .map_err(|_| self.budget_exceeded())?;
            // SAFETY: This unchanged TextRef emits exactly len_units members, reserved locally above.
            units.extend(text.units());
            return Ok(units);
        }
        use poirot_coercionv2::compatibility::{DeferredText, ScalarRef};
        let text = poirot_coercionv2::compatibility::coercev2_text_argument(
            ScalarRef::from(value),
            false,
            |request| match request {
                DeferredText::Number(number) => {
                    self.coercev2_number_to_text(number, NumericPolicy::PinnedPoi)
                }
                DeferredText::Missing => Err(CellError::Value.into()),
                DeferredText::Text(text) => self.project_text(text),
            },
        )?;
        let units = text.encode_utf16().count();
        self.check_text_capacity(units.checked_mul(2).ok_or_else(|| self.budget_exceeded())?)?;
        self.check_values_capacity(units)?;
        self.charge_work(units as u64)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(units)
            .map_err(|_| self.budget_exceeded())?;
        // SAFETY: units was counted from this unchanged string's UTF-16 iterator immediately above.
        output.extend(text.encode_utf16());
        Ok(output)
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
        _policy: NumericPolicy,
    ) -> FunctionResult<String> {
        // Binary64 rendering has fixed precision and bounded scratch. Capacity
        // limits concern the produced buffer, not the formatter's static bound.
        let rendered =
            poirot_coercionv2::compatibility::coercev2_render_finite_number(value, || {
                self.charge_work(1)
            })?;
        self.check_text(rendered.len())?;
        Ok(rendered)
    }
    /// Compare scalars using pinned equality and preserve operational failures.
    pub(crate) fn equal(
        &mut self,
        left: &CellValue,
        right: &CellValue,
        policy: NumericPolicy,
    ) -> FunctionResult<bool> {
        self.session.equal_scalars(left, right, policy)
    }
}

impl Session<'_> {
    /// Shared scalar equality for deferred and elementwise argument consumers.
    pub(crate) fn equal_scalars(
        &mut self,
        left: &CellValue,
        right: &CellValue,
        policy: NumericPolicy,
    ) -> FunctionResult<bool> {
        if policy == NumericPolicy::PinnedPoi {
            // TODO(coercion-non-argument): [LOW] Equality collapses numeric/blank pairs; reusing those values outside comparison would alter type-sensitive behavior.
            let numeric = coercev2_equality_numeric_pair(left, right);
            if let Some((left, right)) = numeric {
                self.budget.charge(WorkKind::HelperOperation, 1)?;
                return Ok(poirot_numeric::compare(left, right)?.is_eq());
            }
        }
        self.budget.charge(WorkKind::HelperOperation, 1)?;
        Ok(compare(left, right, &mut self.budget)?.is_eq())
    }
}

/// Pinned numeric equality admits blanks as zero; other type pairs remain typed.
fn coercev2_equality_numeric_pair(left: &CellValue, right: &CellValue) -> Option<(f64, f64)> {
    poirot_coercionv2::compatibility::coercev2_equality_numeric_pair(left, right)
}
