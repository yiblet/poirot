//! Scalar Unicode projections have an operational failure boundary.
//! UTF16 unit consumers must use text_ref directly rather than this scalar view.
use super::*;
use poirot_model::TextRef;
use std::{borrow::Cow, cmp::Ordering};

fn unsupported(detail: &'static str) -> FunctionFailure {
    EvaluationFailure::UnsupportedCapability {
        capability: CapabilityId::FunctionAdapter,
        source: None,
        detail: detail.into(),
    }
    .into()
}
/// Only callers that need Unicode scalar semantics request this projection.
/// Admission precedes every UTF16 -> UTF8 allocation; no replacement is used.
pub(super) fn checked_text_view<'a>(
    text: TextRef<'a>,
    budget: &mut BudgetMeter,
) -> FunctionResult<Cow<'a, str>> {
    match text {
        TextRef::Utf8(text) => {
            budget.text(text.len())?;
            budget.charge(WorkKind::TextByte, text.len() as u64)?;
            Ok(Cow::Borrowed(text))
        }
        TextRef::Utf16(units) => {
            budget.charge(WorkKind::TextByte, units.len() as u64)?;
            let bytes = TextRef::Utf16(units).utf8_len().map_err(|_| {
                unsupported("Unicode scalar consumer cannot project isolated UTF16 surrogates")
            })?;
            budget.text(bytes)?;
            budget.allocation(units.len())?;
            budget.charge(WorkKind::TextByte, bytes as u64)?;
            String::from_utf16(units).map(Cow::Owned).map_err(|_| {
                unsupported("Unicode scalar consumer cannot project isolated UTF16 surrogates")
            })
        }
    }
}
/// Compare scalars using source-compatible typed ordering and numeric precision.
pub(super) fn source_compare(
    a: &CellValue,
    b: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<Ordering> {
    if let Error(error) = a {
        return Err((*error).into());
    }
    if let Error(error) = b {
        return Err((*error).into());
    }
    // Type dispatch precedes numeric special-exponent validation.
    if matches!(a, Number(_) | Blank) && matches!(b, Number(_) | Blank) {
        for (argument, value) in [(0, a), (1, b)] {
            if let Number(number) = value
                && !number.is_finite()
            {
                return Err(EvaluationFailure::KnownSourceException {
                    source: None,
                    exception: poirot_model::KnownSourceException::SpecialDoubleComparison {
                        argument,
                        bits: poirot_model::IeeeNumberBits::from_number(*number),
                    },
                }
                .into());
            }
        }
        let (a, b) = (
            coercev2_arithmetic_number(a, budget)?,
            coercev2_arithmetic_number(b, budget)?,
        );
        if a == 0.0 && b == 0.0 {
            return Ok(b.is_sign_negative().cmp(&a.is_sign_negative()));
        }
        return source_arithmetic::compare(a, b, budget);
    }
    if matches!(a, Blank) || matches!(b, Blank) {
        return compare(a, b, budget);
    }
    if rank(a) != rank(b) {
        return Ok(rank(a).cmp(&rank(b)));
    }
    compare(a, b, budget)
}
/// Coerce a scalar to a number under shared metered conversion rules.
pub(super) fn coercev2_arithmetic_number(
    v: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<f64> {
    use poirot_coercionv2::compatibility::{
        DeferredNumber, NumberPolicy, coercev2_arithmetic_text, coercev2_number,
    };
    coercev2_number(v.into(), NumberPolicy::Finite, |input| match input {
        DeferredNumber::Text(t) => {
            coercev2_arithmetic_text(&checked_text_view(t, budget)?).map_err(Into::into)
        }
        DeferredNumber::Missing => Err(VALUE_ERROR.into()),
    })
}
/// Boolean text predicates operate on units; malformed UTF16 is an ordinary
/// nonboolean string here, not a failed scalar projection.
pub(super) fn coercev2_logical_boolean(v: &CellValue) -> Result<bool, CellError> {
    poirot_coercionv2::compatibility::coercev2_boolean(
        v.into(),
        poirot_coercionv2::compatibility::BooleanPolicy {
            reject_nan: false,
            blank: true,
        },
    )?
    .ok_or(VALUE_ERROR)
}
/// Source OperandResolver consumers reject NaN; lookup range flags keep coercev2_logical_boolean().
pub(super) fn coercev2_operand_boolean(v: &CellValue) -> Result<bool, CellError> {
    poirot_coercionv2::compatibility::coercev2_boolean(
        v.into(),
        poirot_coercionv2::compatibility::BooleanPolicy {
            reject_nan: true,
            blank: true,
        },
    )?
    .ok_or(VALUE_ERROR)
}
/// Render a scalar as checked helper text without replacing isolated UTF-16 surrogates.
pub(super) fn coercev2_formula_text(
    v: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<String> {
    if let Some(text) = v.text_ref() {
        let text = checked_text_view(text, budget)?;
        // Borrowed UTF8 clones only after the same checked bound. Owned projected
        // buffers transfer ownership without making another copy.
        return Ok(text.into_owned());
    }
    // Scalar formatting is bounded by binary64 precision; admit the actual
    // output rather than charging the maximum internal formatter workspace.
    budget.charge(WorkKind::HelperOperation, 1)?;
    let rendered =
        poirot_coercionv2::compatibility::coercev2_text_with(v.into(), |request| match request {
            poirot_coercionv2::compatibility::DeferredText::Number(n) => {
                Ok(poirot_coercionv2::compatibility::coercev2_number_to_text(n))
            }
            _ => Err(FunctionFailure::from(VALUE_ERROR)),
        })?;
    budget.text(rendered.len())?;
    Ok(rendered)
}
fn rank(v: &CellValue) -> u8 {
    match v {
        Number(_) | Blank => 0,
        Text(_) | CellValue::TextUtf16(_) => 1,
        Boolean(_) => 2,
        Error(_) => 0,
    }
}
/// Compare scalar values under the shared spreadsheet comparison contract.
pub(super) fn compare(
    a: &CellValue,
    b: &CellValue,
    budget: &mut BudgetMeter,
) -> FunctionResult<Ordering> {
    if matches!(a, Number(n) if !n.is_finite()) || matches!(b, Number(n) if !n.is_finite()) {
        return Err(Num.into());
    }
    if let Error(e) = a {
        return Err((*e).into());
    }
    if let Error(e) = b {
        return Err((*e).into());
    }
    // Blank normalization uses borrowed values rather than cloning operand text.
    let zero = Number(0.);
    let empty = Text(String::new());
    let false_value = Boolean(false);
    let a = coercev2_comparison_blank(a, b, &zero, &empty, &false_value);
    let b = coercev2_comparison_blank(b, a, &zero, &empty, &false_value);
    if let (Some(a), Some(b)) = (a.text_ref(), b.text_ref()) {
        budget.check_text(a.storage_bytes())?;
        budget.check_text(b.storage_bytes())?;
        let units = a
            .storage_bytes()
            .checked_add(b.storage_bytes())
            .ok_or_else(|| unsupported("comparison work size overflow"))?;
        budget.charge(WorkKind::Comparison, units as u64)?;
        return Ok(poirot_text::fold::compare_utf16(a.units(), b.units()));
    }
    Ok(match (a, b) {
        (Number(a), Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
        (Boolean(a), Boolean(b)) => a.cmp(b),
        _ => rank(a).cmp(&rank(b)),
    })
}
/// EvalResult callers retain their semantic match arms; operational failures
/// escape before IFERROR/ISERROR and cannot become a cell value.
pub(super) fn semantic<T>(result: FunctionResult<T>) -> EvalResult<Result<T, CellError>> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(Err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
/// Replace only spreadsheet failures, preserving operational aborts unchanged.
pub(super) fn remap_semantic(error: FunctionFailure, replacement: CellError) -> FunctionFailure {
    match error {
        FunctionFailure::Spreadsheet(_) => replacement.into(),
        other => other,
    }
}

/// Format syntax is semantic; fidelity and renderer resource causes are typed
/// operational failures. Renderer errors never invent BudgetFailure amounts.
pub(super) fn format_failure(error: poirot_format::FormatError) -> FunctionFailure {
    use poirot_format::FormatErrorKind::*;
    match error.kind {
        InvalidFormat => VALUE_ERROR.into(),
        UnsupportedFidelity => EvaluationFailure::UnsupportedCapability {
            capability: CapabilityId::Formatting,
            source: None,
            detail: error.to_string(),
        }
        .into(),
        ResourceLimit => EvaluationFailure::ResourceLimit {
            capability: CapabilityId::Formatting,
            source: None,
            detail: error.to_string(),
        }
        .into(),
    }
}

/// Comparison converts blanks using the counterpart's type, without parsing text.
fn coercev2_comparison_blank<'a>(
    value: &'a CellValue,
    counterpart: &CellValue,
    zero: &'a CellValue,
    empty: &'a CellValue,
    false_value: &'a CellValue,
) -> &'a CellValue {
    poirot_coercionv2::compatibility::coercev2_comparison_blank(
        value,
        counterpart,
        zero,
        empty,
        false_value,
    )
}

#[cfg(test)]
#[path = "core_projection_tests.rs"]
mod tests;

impl Session<'_> {
    /// Parse admitted numeric text with explicit locale and metered grammar work.
    pub(super) fn coercev2_formatted_text(
        &mut self,
        text: &str,
    ) -> FunctionResult<poirot_coercionv2::compatibility::NumericTextDecision> {
        self.read(DependencyRead::NumberLocale);
        poirot_coercionv2::coercev2_formatted_number(text, self.number_locale, |bytes, work| {
            self.budget.text(bytes)?;
            self.budget.charge(WorkKind::TextByte, work)?;
            Ok::<_, FunctionFailure>(())
        })
    }
    /// Scalar admission converts only when an argument requests a number.
    pub(super) fn coercev2_scalar_number(&mut self, value: &CellValue) -> FunctionResult<f64> {
        use poirot_coercionv2::compatibility::{
            DeferredNumber, NumberPolicy, NumericTextDecision, coercev2_number,
        };
        coercev2_number(
            value.into(),
            NumberPolicy::Finite,
            |request| match request {
                DeferredNumber::Text(text) => {
                    let text = checked_text_view(text, &mut self.budget)?;
                    match self.coercev2_formatted_text(&text)? {
                        NumericTextDecision::Number(result) => result.map_err(Into::into),
                        NumericTextDecision::DateTime => self.coercev2_numeric_date_time(&text),
                    }
                }
                DeferredNumber::Missing => Err(VALUE_ERROR.into()),
            },
        )
    }
    /// Existing numeric operator inputs retain their bits under the pinned policy.
    pub(super) fn coercev2_operator_number(&mut self, value: &CellValue) -> FunctionResult<f64> {
        if let Number(n) = value {
            return Ok(*n);
        }
        self.coercev2_scalar_number(value)
    }
}

impl Session<'_> {
    fn record_numeric_date_effects(
        &mut self,
        effects: poirot_coercionv2::date_time::Effects,
        key: Option<(usize, CellAddress)>,
    ) -> FunctionResult<()> {
        if effects == poirot_coercionv2::date_time::Effects::default() {
            return Ok(());
        }
        use crate::run::{ContextRead, DependencyRead};
        if effects.default_year {
            self.read(DependencyRead::ContextField(ContextRead::AsOf));
            self.read(DependencyRead::ContextField(ContextRead::UtcOffset));
        }
        if effects.timezone {
            self.read(DependencyRead::ContextField(ContextRead::UtcOffset));
        }
        // Conservative locale category; coercion retains its pinned locale.
        if effects.locale {
            self.read(DependencyRead::ContextField(ContextRead::DateOrder));
        }
        let Some(key) = key else { return Ok(()) };
        if !self.numeric_date_effects.contains_key(&key) {
            self.budget.check_allocation(
                self.numeric_date_effects
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| {
                        unsupported_numeric_date(
                            CapabilityId::DateParsing,
                            "numeric date dependency size overflow",
                        )
                    })?,
            )?;
            self.budget.charge(WorkKind::Materialization, 1)?;
        }
        let accumulated = self.numeric_date_effects.entry(key).or_default();
        accumulated.default_year |= effects.default_year;
        accumulated.locale |= effects.locale;
        accumulated.timezone |= effects.timezone;
        Ok(())
    }
    pub(super) fn coercev2_numeric_date_time(&mut self, text: &str) -> FunctionResult<f64> {
        self.coercev2_numeric_date_time_at(text, self.consumer.map(|t| (t.sheet, t.address)))
    }
    pub(super) fn coercev2_numeric_date_time_at(
        &mut self,
        text: &str,
        key: Option<(usize, CellAddress)>,
    ) -> FunctionResult<f64> {
        use poirot_coercionv2::date_time::{Context, LocalePolicy, Outcome, TimezonePolicy};
        if self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation {
            use poirot_coercionv2::date_time::{
                CalendarTextPlan, Effects, coercev2_calendar_text_plan,
            };
            self.budget
                .charge(WorkKind::HelperOperation, text.len() as u64)?;
            if let CalendarTextPlan::Civil { date, fraction } = coercev2_calendar_text_plan(text)? {
                self.read(DependencyRead::ContextField(
                    crate::run::ContextRead::DateSystem,
                ));
                self.record_numeric_date_effects(
                    Effects {
                        locale: date.reads_date_order,
                        ..Effects::default()
                    },
                    key,
                )?;
                let order = match self.context.localization.date_order {
                    DateOrder::MonthDayYear => poirot_coercionv2::dates::DateOrder::MonthDayYear,
                    DateOrder::DayMonthYear => poirot_coercionv2::dates::DateOrder::DayMonthYear,
                };
                let date = date.coercev2_resolve(order, || {
                    self.record_numeric_date_effects(
                        Effects {
                            default_year: true,
                            ..Effects::default()
                        },
                        key,
                    )?;
                    self.numeric_default_year().map(i64::from)
                })?;
                use poirot_coercionv2::date_time::{CalendarEpoch, coercev2_calendar_serial};
                let epoch = match self.workbook.date_system {
                    poirot_model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
                    poirot_model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
                };
                return Ok(coercev2_calendar_serial(date, epoch)? + fraction);
            }
        }

        let mut context = Context {
            // Sentinel avoids even consulting the frozen clock for explicit-year
            // dates, standalone times, and ordinary rejected strings.
            default_year: 0,
            locale: LocalePolicy::English,
            timezone: TimezonePolicy::FixedOffsetSeconds(
                self.context.localization.utc_offset_seconds,
            ),
        };
        self.budget
            .charge(WorkKind::HelperOperation, text.len() as u64)?;
        let (mut outcome, effects) =
            poirot_coercionv2::date_time::coercev2_date_time_with_effects(text, context);
        self.record_numeric_date_effects(effects, key)?;
        if matches!(
            outcome,
            Outcome::Unsupported("default year outside proven calendar range")
        ) {
            context.default_year = self.numeric_default_year()?;
            self.budget
                .charge(WorkKind::HelperOperation, text.len() as u64)?;
            let (resolved, effects) =
                poirot_coercionv2::date_time::coercev2_date_time_with_effects(text, context);
            self.record_numeric_date_effects(effects, key)?;
            outcome = resolved;
        }
        match outcome {
            Outcome::Valid { value, .. } => Ok(value),
            Outcome::Rejected => Err(VALUE_ERROR.into()),
            Outcome::Unsupported(detail) => {
                Err(unsupported_numeric_date(CapabilityId::DateParsing, detail))
            }
        }
    }
    fn numeric_default_year(&self) -> FunctionResult<i32> {
        let context = self.context;
        let seconds = crate::clock::unix_seconds(context.as_of_time)
            + f64::from(context.localization.utc_offset_seconds);
        let days = (seconds / 86400.0).floor();
        // Bound the civil algorithm before any float/integer conversion.
        // These endpoints are Gregorian 0001-01-01 and 10001-01-01.
        if !(-719162.0..2933263.0).contains(&days) {
            return Err(unsupported_numeric_date(
                CapabilityId::DateParsing,
                "numeric date default year requires a supported frozen clock",
            ));
        }
        // Gregorian conversion adapts Howard Hinnant's days_from_civil/civil_from_days.
        // MIT reference: HowardHinnant/date 5bdb7e6f31fac909c090a46dbd9fea27b6e609a4,
        // include/date/date.h; see root LICENSES/ACKNOWLEDGEMENTS.
        let z = days as i64 + 719468;
        let era = z.div_euclid(146097);
        let day_of_era = z - era * 146097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let month = (5 * day_of_year + 2) / 153;
        let year = year_of_era + era * 400 + i64::from(month >= 10);
        if !(1..=10000).contains(&year) {
            return Err(unsupported_numeric_date(
                CapabilityId::DateParsing,
                "numeric date default year outside supported calendar",
            ));
        }
        Ok(year as i32)
    }
}

fn unsupported_numeric_date(capability: CapabilityId, detail: &'static str) -> FunctionFailure {
    EvaluationFailure::UnsupportedCapability {
        capability,
        source: None,
        detail: detail.into(),
    }
    .into()
}
