//! Numeric and text coercion contracts, including attributed context effects.
use super::call_host::CallHost;
use crate::eval::config::calendar::valid;
use crate::eval::limits::WorkKind;
use crate::eval::session::Coercer;
use crate::eval::values::core_projection::{
    checked_text_view, coerce_arithmetic_number, coerce_formula_text, compare,
};
use crate::eval::values::{FunctionError, FunctionResult};
use crate::model::CellValue::Blank;
use crate::model::{CellError, CellValue};
use poirot_functions::coercion::compatibility::NumericTextDecision;

/// Coerces a call's scalars to numbers, text and equality under its numeric policy.
pub(in crate::eval) struct ArgumentCoercion<'h, 'call, 'workbook> {
    /// The call whose budget and context the coercions charge and record.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> ArgumentCoercion<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's coercions.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Coerce a calendar serial under the existing date-family rules.
    pub(in crate::eval) fn coerce_calendar_date(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<f64> {
        let n = if let Some(text) = value.text_ref() {
            let text = checked_text_view(text, &mut self.host.session.state.runtime.budget)?;
            match Coercer::new(self.host.session).coerce_formatted_text(&text)? {
                NumericTextDecision::Number(number) => number?,
                NumericTextDecision::DateTime => {
                    Coercer::new(self.host.session).coerce_numeric_date_time(&text)?
                }
            }
        } else {
            Coercer::new(self.host.session).coerce_scalar_number(value)?
        };
        Ok(valid(n, self.host.session.sources.workbook.date_system)?)
    }

    /// Coerce a scalar using shared numeric conversion and attributed date fallback.
    pub(in crate::eval) fn coerce_scalar_number(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<f64> {
        if let Some(text) = value.text_ref() {
            let text = checked_text_view(text, &mut self.host.session.state.runtime.budget)?;
            self.coerce_number_text(&text)
        } else {
            coerce_arithmetic_number(value, &mut self.host.session.state.runtime.budget)
        }
    }

    /// Render a nonmissing scalar losslessly for UTF-16 text consumers.
    pub(in crate::eval) fn coerce_text_units(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<Vec<u16>> {
        if *value == Blank {
            return Ok(Vec::new());
        }
        if let Some(text) = value.text_ref() {
            self.host.check_text_capacity(
                text.len_units()
                    .checked_mul(2)
                    .ok_or_else(|| self.host.budget_exceeded())?,
            )?;
            self.host.check_values_capacity(text.len_units())?;
            self.host.charge_work(text.len_units() as u64)?;
            let mut units = Vec::new();
            units
                .try_reserve_exact(text.len_units())
                .map_err(|reservation_source| self.host.reservation_failure(reservation_source))?;
            // SAFETY: This unchanged TextRef emits exactly len_units members, reserved locally above.
            units.extend(text.units());
            return Ok(units);
        }
        use poirot_functions::coercion::compatibility::{DeferredText, ScalarRef};
        let text = poirot_functions::coercion::compatibility::coerce_text_argument(
            ScalarRef::from(value),
            false,
            |request| match request {
                DeferredText::Number(number) => self.coerce_number_to_text(number),
                DeferredText::Missing => Err(CellError::Value.into()),
                DeferredText::Text(text) => self.project_text(text),
            },
        )?;
        let units = text.encode_utf16().count();
        self.host.check_text_capacity(
            units
                .checked_mul(2)
                .ok_or_else(|| self.host.budget_exceeded())?,
        )?;
        self.host.check_values_capacity(units)?;
        self.host.charge_work(units as u64)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(units)
            .map_err(|reservation_source| self.host.reservation_failure(reservation_source))?;
        // SAFETY: units was counted from this unchanged string's UTF-16 iterator immediately above.
        output.extend(text.encode_utf16());
        Ok(output)
    }

    /// Render a scalar under shared text coercion and allocation limits.
    pub(in crate::eval) fn coerce_scalar_text(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<String> {
        coerce_formula_text(value, &mut self.host.session.state.runtime.budget)
    }

    /// Project model text into a checked owned helper string.
    pub(in crate::eval) fn project_text(
        &mut self,
        value: crate::model::TextRef<'_>,
    ) -> FunctionResult<String> {
        checked_text_view(value, &mut self.host.session.state.runtime.budget)
            .map(|text| text.into_owned())
    }

    /// Parse numeric text, recording context consumed by any date-time fallback.
    pub(in crate::eval) fn coerce_number_text(&mut self, text: &str) -> FunctionResult<f64> {
        self.host.check_text(text.len())?;
        let decision = Coercer::new(self.host.session).coerce_formatted_text(text)?;
        match decision {
            NumericTextDecision::Number(result) => result.map_err(Into::into),
            NumericTextDecision::DateTime => Coercer::new(self.host.session)
                .coerce_numeric_date_time_at(text, Some((self.host.sheet, self.host.address))),
        }
    }

    /// Render a number with the selected source-compatible numeric formatter.
    pub(in crate::eval) fn coerce_number_to_text(&mut self, value: f64) -> FunctionResult<String> {
        // Binary64 rendering has fixed precision and bounded scratch. Capacity
        // limits concern the produced buffer, not the formatter's static bound.
        let rendered =
            poirot_functions::coercion::compatibility::coerce_render_finite_number(value, || {
                self.host.charge_work(1).map_err(FunctionError::Operational)
            })?;
        self.host.check_text(rendered.len())?;
        Ok(rendered)
    }

    /// Compare scalars using pinned equality and preserve operational failures.
    pub(in crate::eval) fn equal(
        &mut self,
        left: &CellValue,
        right: &CellValue,
    ) -> FunctionResult<bool> {
        {
            // TODO(coercion-non-argument): [LOW] Equality collapses numeric/blank pairs; reusing those values outside comparison would alter type-sensitive behavior.
            let numeric = coerce_equality_numeric_pair(left, right);
            if let Some((left, right)) = numeric {
                self.host
                    .session
                    .state
                    .runtime
                    .budget
                    .charge(WorkKind::HelperOperation, 1)?;
                return Ok(poirot_functions::numeric::compare(left, right)?.is_eq());
            }
        }
        self.host
            .session
            .state
            .runtime
            .budget
            .charge(WorkKind::HelperOperation, 1)?;
        Ok(compare(left, right, &mut self.host.session.state.runtime.budget)?.is_eq())
    }

    /// Borrow Unicode scalar text, allocating only when UTF-16 projection requires it.
    pub(in crate::eval) fn scalar_text_view<'a>(
        &mut self,
        text: crate::model::TextRef<'a>,
    ) -> FunctionResult<std::borrow::Cow<'a, str>> {
        checked_text_view(text, &mut self.host.session.state.runtime.budget)
    }

    /// Parse scalar text through the legacy, consumer-attributed calendar path.
    pub(in crate::eval) fn legacy_numeric_date_time(&mut self, text: &str) -> FunctionResult<f64> {
        Coercer::new(self.host.session).coerce_numeric_date_time(text)
    }

    /// Parse formatted numeric text with the evaluator's locale and work charges.
    pub(in crate::eval) fn formatted_number_text(
        &mut self,
        text: &str,
    ) -> FunctionResult<poirot_functions::coercion::compatibility::NumericTextDecision> {
        Coercer::new(self.host.session).coerce_formatted_text(text)
    }
}

/// Pinned numeric equality admits blanks as zero; other type pairs remain typed.
fn coerce_equality_numeric_pair(left: &CellValue, right: &CellValue) -> Option<(f64, f64)> {
    poirot_functions::coercion::compatibility::coerce_equality_numeric_pair(left, right)
}
