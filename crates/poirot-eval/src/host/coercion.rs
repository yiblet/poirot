//! Numeric and text coercion contracts, including attributed context effects.
use super::*;

impl CallHost<'_, '_> {
    /// Resolve and numerically coerce one argument under explicit missing and numeric rules.
    pub(crate) fn number(
        &mut self,
        index: usize,
        missing: MissingPolicy,
        policy: NumericPolicy,
    ) -> FunctionResult<f64> {
        let value = self.scalar(index, missing)?;
        match value {
            Text(text) => self.coerce_number_text(&text, policy),
            value => self.scalar_number(&value, policy),
        }
    }
    /// Coerce a scalar using shared numeric conversion and attributed date fallback.
    pub(crate) fn scalar_number(
        &mut self,
        value: &CellValue,
        policy: NumericPolicy,
    ) -> FunctionResult<f64> {
        if let Some(text) = value.text_ref() {
            let text = checked_text_view(text, &mut self.session.budget)?;
            self.coerce_number_text(&text, policy)
        } else {
            number(value, &mut self.session.budget)
        }
    }
    /// Render a scalar under shared text coercion and allocation limits.
    pub(crate) fn scalar_text(&mut self, value: &CellValue) -> FunctionResult<String> {
        text(value, &mut self.session.budget)
    }
    /// Project model text into a checked owned helper string.
    pub(crate) fn project_text(
        &mut self,
        value: poirot_model::TextRef<'_>,
    ) -> FunctionResult<String> {
        checked_text_view(value, &mut self.session.budget).map(|text| text.into_owned())
    }
    /// Parse numeric text, recording context consumed by any date-time fallback.
    pub(crate) fn coerce_number_text(
        &mut self,
        text: &str,
        policy: NumericPolicy,
    ) -> FunctionResult<f64> {
        self.check_text(text.len())?;
        match numeric_policy::decide(text, policy) {
            numeric_policy::CoercionDecision::Number(result) => result.map_err(Into::into),
            numeric_policy::CoercionDecision::DateTime => self.numeric_date_time(text),
        }
    }
    fn record_numeric_date_effects(
        &mut self,
        effects: poirot_coercion::Effects,
    ) -> FunctionResult<()> {
        if effects == poirot_coercion::Effects::default() {
            return Ok(());
        }
        use crate::run::{ContextRead, DependencyRead};
        if effects.default_year {
            self.session
                .read(DependencyRead::ContextField(ContextRead::AsOf));
            self.session
                .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        }
        if effects.timezone {
            self.session
                .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        }
        // Conservative locale category; coercion retains its pinned locale.
        if effects.locale {
            self.session
                .read(DependencyRead::ContextField(ContextRead::DateOrder));
        }
        let key = (self.sheet, self.address);
        if !self.session.numeric_date_effects.contains_key(&key) {
            self.session
                .budget
                .check_allocation(self.session.numeric_date_effects.len() + 1)?;
            self.session.budget.charge(WorkKind::Materialization, 1)?;
        }
        let accumulated = self.session.numeric_date_effects.entry(key).or_default();
        accumulated.default_year |= effects.default_year;
        accumulated.locale |= effects.locale;
        accumulated.timezone |= effects.timezone;
        Ok(())
    }
    fn numeric_date_time(&mut self, text: &str) -> FunctionResult<f64> {
        use poirot_coercion::{Context, LocalePolicy, Outcome, TimezonePolicy};
        let mut context = Context {
            // Sentinel avoids even consulting the frozen clock for explicit-year
            // dates, standalone times, and ordinary rejected strings.
            default_year: 0,
            locale: LocalePolicy::English,
            timezone: TimezonePolicy::FixedOffsetSeconds(
                self.session.context.localization.utc_offset_seconds,
            ),
        };
        self.charge_work(text.len() as u64)?;
        let (mut outcome, effects) = poirot_coercion::parse_date_time_with_effects(text, context);
        self.record_numeric_date_effects(effects)?;
        if matches!(
            outcome,
            Outcome::Unsupported("default year outside proven calendar range")
        ) {
            context.default_year = self.numeric_default_year()?;
            self.charge_work(text.len() as u64)?;
            let (resolved, effects) = poirot_coercion::parse_date_time_with_effects(text, context);
            self.record_numeric_date_effects(effects)?;
            outcome = resolved;
        }
        match outcome {
            Outcome::Valid { value, .. } => Ok(value),
            Outcome::Rejected => Err(VALUE_ERROR.into()),
            Outcome::Unsupported(detail) => {
                Err(self.unsupported(CapabilityId::DateParsing, detail))
            }
        }
    }
    fn numeric_default_year(&self) -> FunctionResult<i32> {
        let context = self.session.context;
        let seconds = crate::clock::unix_seconds(context.as_of_time)
            + f64::from(context.localization.utc_offset_seconds);
        let days = (seconds / 86400.0).floor();
        // Bound the civil algorithm before any float/integer conversion.
        // These endpoints are Gregorian 0001-01-01 and 10001-01-01.
        if !(-719162.0..2933263.0).contains(&days) {
            return Err(self.unsupported(
                CapabilityId::DateParsing,
                "numeric date default year requires a supported frozen clock",
            ));
        }
        let z = days as i64 + 719468;
        let era = z.div_euclid(146097);
        let day_of_era = z - era * 146097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month = (5 * day_of_year + 2) / 153;
        let year = year_of_era + era * 400 + i64::from(month >= 10);
        if !(1..=10000).contains(&year) {
            return Err(self.unsupported(
                CapabilityId::DateParsing,
                "numeric date default year outside supported calendar",
            ));
        }
        Ok(year as i32)
    }
    /// Render a number with the selected source-compatible numeric formatter.
    pub(crate) fn number_text(
        &mut self,
        value: f64,
        policy: NumericPolicy,
    ) -> FunctionResult<String> {
        if !value.is_finite() {
            return Err(Num.into());
        }
        match policy {
            #[cfg(test)]
            NumericPolicy::LegacyCompatible => {
                self.check_text(poirot_numeric::number_text::ALLOCATION_BOUND)?;
                Ok(poirot_numeric::number_text::render(
                    value,
                    poirot_numeric::number_text::SourceVersion::PoirotF6c572d,
                ))
            }
            NumericPolicy::PinnedPoi => {
                self.check_text(poirot_numeric::number_text::ALLOCATION_BOUND)?;
                Ok(poirot_numeric::number_text::render(
                    value,
                    poirot_numeric::number_text::SourceVersion::Poi7321209,
                ))
            }
        }
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
            let numeric = match (left, right) {
                (Number(left), Number(right)) => Some((*left, *right)),
                (Blank, Number(right)) => Some((0., *right)),
                (Number(left), Blank) => Some((*left, 0.)),
                _ => None,
            };
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
