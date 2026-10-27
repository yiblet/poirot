//! Context-dependent scalar coercion: locale numbers, numeric dates and their dependencies.

use crate::eval::config::DateOrder;
use crate::eval::limits::{CapabilityId, EvaluationError, WorkKind};
use crate::eval::ports::DependencyRead;
use crate::eval::session::composition::Session;
use crate::eval::session::reads::ReadRecorder;
use crate::eval::values::core_projection::checked_text_view;
use crate::eval::values::{FunctionError, FunctionResult, VALUE_ERROR};
use crate::model::CellValue::Number;
use crate::model::{CellAddress, CellValue};

/// Coerces scalars to numbers using run policies, recording the context fields they read.
pub(in crate::eval) struct Coercer<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> Coercer<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Parse admitted numeric text with explicit locale and metered grammar work.
    pub(in crate::eval) fn coerce_formatted_text(
        &mut self,
        text: &str,
    ) -> FunctionResult<poirot_functions::coercion::compatibility::NumericTextDecision> {
        ReadRecorder::new(self.session).read(DependencyRead::NumberLocale);
        poirot_functions::coercion::coerce_formatted_number(
            text,
            self.session.policies.number_locale,
            |bytes, work| {
                self.session.state.runtime.budget.text(bytes)?;
                self.session
                    .state
                    .runtime
                    .budget
                    .charge(WorkKind::TextByte, work)?;
                Ok::<_, FunctionError>(())
            },
        )
    }

    /// Scalar admission converts only when an argument requests a number.
    pub(in crate::eval) fn coerce_scalar_number(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<f64> {
        use poirot_functions::coercion::compatibility::{
            DeferredNumber, NumberPolicy, NumericTextDecision, coerce_number,
        };
        coerce_number(
            value.into(),
            NumberPolicy::Finite,
            |request| match request {
                DeferredNumber::Text(text) => {
                    let text = checked_text_view(text, &mut self.session.state.runtime.budget)?;
                    match self.coerce_formatted_text(&text)? {
                        NumericTextDecision::Number(result) => result.map_err(Into::into),
                        NumericTextDecision::DateTime => self.coerce_numeric_date_time(&text),
                    }
                }
                DeferredNumber::Missing => Err(VALUE_ERROR.into()),
            },
        )
    }

    /// Preserve numeric operand bits before the operator evaluates them.
    pub(in crate::eval) fn coerce_operator_number(
        &mut self,
        value: &CellValue,
    ) -> FunctionResult<f64> {
        if let Number(n) = value {
            return Ok(*n);
        }
        self.coerce_scalar_number(value)
    }

    /// Record the context fields a numeric date parse consulted, keyed by consuming cell.
    fn record_numeric_date_effects(
        &mut self,
        effects: poirot_functions::coercion::date_time::Effects,
        key: Option<(usize, CellAddress)>,
    ) -> FunctionResult<()> {
        if effects == poirot_functions::coercion::date_time::Effects::default() {
            return Ok(());
        }
        use crate::eval::ports::{ContextRead, DependencyRead};
        if effects.default_year {
            ReadRecorder::new(self.session).read(DependencyRead::ContextField(ContextRead::AsOf));
            ReadRecorder::new(self.session)
                .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        }
        if effects.timezone {
            ReadRecorder::new(self.session)
                .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        }
        // Conservative locale category; coercion retains its pinned locale.
        if effects.locale {
            ReadRecorder::new(self.session)
                .read(DependencyRead::ContextField(ContextRead::DateOrder));
        }
        let Some(key) = key else { return Ok(()) };
        if !self
            .session
            .state
            .runtime
            .effects
            .numeric_date_effects
            .contains_key(&key)
        {
            self.session.state.runtime.budget.check_allocation(
                self.session
                    .state
                    .runtime
                    .effects
                    .numeric_date_effects
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| {
                        unsupported_numeric_date(
                            CapabilityId::DateParsing,
                            crate::eval::limits::EvaluationReasonError::NumericDateDependencySizeOverflow,
                        )
                    })?,
            )?;
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::Materialization, 1)?;
        }
        let accumulated = self
            .session
            .state
            .runtime
            .effects
            .numeric_date_effects
            .entry(key)
            .or_default();
        accumulated.default_year |= effects.default_year;
        accumulated.locale |= effects.locale;
        accumulated.timezone |= effects.timezone;
        Ok(())
    }

    /// Parse numeric date or time text for the active consumer.
    pub(in crate::eval) fn coerce_numeric_date_time(&mut self, text: &str) -> FunctionResult<f64> {
        self.coerce_numeric_date_time_at(
            text,
            self.session
                .state
                .runtime
                .stack
                .consumer
                .map(|t| (t.sheet, t.address)),
        )
    }

    /// Parse numeric date or time text, attributing context effects to `key`.
    pub(in crate::eval) fn coerce_numeric_date_time_at(
        &mut self,
        text: &str,
        key: Option<(usize, CellAddress)>,
    ) -> FunctionResult<f64> {
        use poirot_functions::coercion::date_time::{
            Context, LocalePolicy, Outcome, TimezonePolicy,
        };
        {
            use poirot_functions::coercion::date_time::{
                CalendarTextPlan, Effects, coerce_calendar_text_plan,
            };
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::HelperOperation, text.len() as u64)?;
            if let CalendarTextPlan::Civil { date, fraction } = coerce_calendar_text_plan(text)? {
                ReadRecorder::new(self.session).read(DependencyRead::ContextField(
                    crate::eval::ports::ContextRead::DateSystem,
                ));
                self.record_numeric_date_effects(
                    Effects {
                        locale: date.reads_date_order,
                        ..Effects::default()
                    },
                    key,
                )?;
                let order = match self.session.policies.context.localization.date_order {
                    DateOrder::MonthDayYear => {
                        poirot_functions::coercion::dates::DateOrder::MonthDayYear
                    }
                    DateOrder::DayMonthYear => {
                        poirot_functions::coercion::dates::DateOrder::DayMonthYear
                    }
                };
                let date = date.coerce_resolve(order, || {
                    self.record_numeric_date_effects(
                        Effects {
                            default_year: true,
                            ..Effects::default()
                        },
                        key,
                    )?;
                    self.numeric_default_year().map(i64::from)
                })?;
                use poirot_functions::coercion::date_time::{
                    CalendarEpoch, coerce_calendar_serial,
                };
                let epoch = match self.session.sources.workbook.date_system {
                    crate::model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
                    crate::model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
                };
                return Ok(coerce_calendar_serial(date, epoch)? + fraction);
            }
        }

        let mut context = Context {
            // Sentinel avoids even consulting the frozen clock for explicit-year
            // dates, standalone times, and ordinary rejected strings.
            default_year: 0,
            locale: LocalePolicy::English,
            timezone: TimezonePolicy::FixedOffsetSeconds(
                self.session
                    .policies
                    .context
                    .localization
                    .utc_offset_seconds,
            ),
        };
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::HelperOperation, text.len() as u64)?;
        let (mut outcome, effects) =
            poirot_functions::coercion::date_time::coerce_date_time_with_effects(text, context);
        self.record_numeric_date_effects(effects, key)?;
        if matches!(
            outcome,
            Outcome::Unsupported(
                poirot_functions::coercion::DateTimeError::DefaultYearOutOfRange { .. }
            )
        ) {
            context.default_year = self.numeric_default_year()?;
            self.session
                .state
                .runtime
                .budget
                .charge(WorkKind::HelperOperation, text.len() as u64)?;
            let (resolved, effects) =
                poirot_functions::coercion::date_time::coerce_date_time_with_effects(text, context);
            self.record_numeric_date_effects(effects, key)?;
            outcome = resolved;
        }
        match outcome {
            Outcome::Valid { value, .. } => Ok(value),
            Outcome::Rejected => Err(VALUE_ERROR.into()),
            Outcome::Unsupported(reason) => Err(unsupported_numeric_date(
                CapabilityId::DateParsing,
                crate::eval::limits::EvaluationReasonError::DateTime(std::sync::Arc::new(reason)),
            )),
        }
    }

    /// The frozen clock's civil year used for dates written without a year.
    fn numeric_default_year(&self) -> FunctionResult<i32> {
        let context = self.session.policies.context;
        let seconds = crate::eval::config::clock::unix_seconds(context.as_of_time)
            + f64::from(context.localization.utc_offset_seconds);
        let days = (seconds / 86400.0).floor();
        // Bound the civil algorithm before any float/integer conversion.
        // These endpoints are Gregorian 0001-01-01 and 10001-01-01.
        if !(-719162.0..2933263.0).contains(&days) {
            return Err(unsupported_numeric_date(
                CapabilityId::DateParsing,
                crate::eval::limits::EvaluationReasonError::NumericDateDefaultYearRequiresASupportedFrozenClock,
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
                crate::eval::limits::EvaluationReasonError::NumericDateDefaultYearOutsideSupportedCalendar,
            ));
        }
        Ok(year as i32)
    }
}

/// An unsupported numeric date capability failure.
fn unsupported_numeric_date(
    capability: CapabilityId,
    reason: crate::eval::limits::EvaluationReasonError,
) -> FunctionError {
    EvaluationError::UnsupportedCapability {
        capability,
        target: None,
        reason,
    }
    .into()
}
