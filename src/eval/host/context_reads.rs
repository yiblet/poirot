//! Captured clock, calendar, locale and currency reads with their recorded context dependencies.
use super::call_host::CallHost;
use crate::eval::config::calendar::{serial, valid};
use crate::eval::functions::registry::FunctionId;
use crate::eval::limits::CapabilityId;
use crate::eval::ports::{ContextRead, DependencyRead};
use crate::eval::session::ReadRecorder;
use crate::eval::values::FunctionResult;
use crate::model::CellError::{self, Num};
use poirot_functions::coercion::dates::calendar::civil;
use std::time::SystemTime;

/// Reads a call's captured context fields (clock, calendar, numeric locale, currency and
/// volatility) and records each dependency it consumes; snapshots inspect a field whose
/// dependency the caller records separately.
pub(in crate::eval) struct ContextReads<'h, 'call, 'workbook> {
    /// The call whose session records the context reads.
    host: &'h mut CallHost<'call, 'workbook>,
}

impl<'h, 'call, 'workbook> ContextReads<'h, 'call, 'workbook> {
    /// Borrow the call host for this executor's reads.
    pub(in crate::eval) fn new(host: &'h mut CallHost<'call, 'workbook>) -> Self {
        Self { host }
    }

    /// Record a volatile builtin dependency for this checked call.
    pub(in crate::eval) fn record_volatile(&mut self, function: FunctionId) {
        ReadRecorder::new(self.host.session).read(DependencyRead::Volatile(function));
    }

    /// Read the captured clock and record the exact context dependency.
    pub(in crate::eval) fn as_of_time(&mut self) -> SystemTime {
        ReadRecorder::new(self.host.session).read(DependencyRead::ContextField(ContextRead::AsOf));
        self.host.session.policies.context.as_of_time
    }

    /// Read the fixed UTC offset and record its context dependency.
    pub(in crate::eval) fn utc_offset_seconds(&mut self) -> i32 {
        ReadRecorder::new(self.host.session)
            .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        self.host
            .session
            .policies
            .context
            .localization
            .utc_offset_seconds
    }

    /// Read the numeric slash-date order and record its context dependency.
    pub(in crate::eval) fn date_order(&mut self) -> crate::eval::config::DateOrder {
        ReadRecorder::new(self.host.session)
            .read(DependencyRead::ContextField(ContextRead::DateOrder));
        self.host.session.policies.context.localization.date_order
    }

    /// Read the workbook date system and record its context dependency.
    pub(in crate::eval) fn selected_date_system(&mut self) -> crate::model::DateSystem {
        ReadRecorder::new(self.host.session)
            .read(DependencyRead::ContextField(ContextRead::DateSystem));
        self.host.session.sources.workbook.date_system
    }

    /// Convert the captured clock through the date family's existing serial rule.
    pub(in crate::eval) fn clock_serial(&mut self) -> Result<f64, CellError> {
        ReadRecorder::new(self.host.session).read(DependencyRead::ContextField(ContextRead::AsOf));
        ReadRecorder::new(self.host.session)
            .read(DependencyRead::ContextField(ContextRead::UtcOffset));
        let o = self.host.session.policies.context;
        if !(-86400..=86400).contains(&o.localization.utc_offset_seconds) {
            return Err(Num);
        }
        let sec = crate::eval::config::clock::unix_seconds(o.as_of_time)
            + f64::from(o.localization.utc_offset_seconds);
        let days = (sec / 86400.).floor();
        if !(-25568. ..2_932_897.).contains(&days) {
            return Err(Num);
        }
        let d = civil(days as i64);
        ReadRecorder::new(self.host.session)
            .read(DependencyRead::ContextField(ContextRead::DateSystem));
        valid(
            serial(d, self.host.session.sources.workbook.date_system)?
                + sec.rem_euclid(86400.) / 86400.,
            self.host.session.sources.workbook.date_system,
        )
    }

    /// Record an argument consumer's dependence on numeric locale defaults.
    pub(in crate::eval) fn read_number_locale(&mut self) {
        ReadRecorder::new(self.host.session).read(DependencyRead::NumberLocale);
    }

    /// Inspect the captured numeric locale before a helper requests its dependency.
    pub(in crate::eval) fn number_locale_snapshot(
        &self,
    ) -> &'workbook poirot_parsing::number::NumberLocale {
        self.host.session.policies.number_locale
    }

    /// Inspect formatter context without adding a read for nondate formats.
    pub(in crate::eval) fn date_system_snapshot(&self) -> crate::model::DateSystem {
        self.host.session.sources.workbook.date_system
    }

    /// Record formatting consumption and copy explicitly configured currency settings.
    pub(in crate::eval) fn currency_context(
        &mut self,
    ) -> FunctionResult<crate::eval::config::CurrencyContext> {
        ReadRecorder::new(self.host.session).read(DependencyRead::Formatting);
        let context = self.host.session.policies.currency.ok_or_else(|| {
            self.host.unsupported(
                CapabilityId::Formatting,
                crate::eval::limits::EvaluationReasonError::CurrencyLocalizationIsNotDeclaredByTheCapturedRunConfiguration,
            )
        })?;
        // Both the clone and renderer are bounded before any owned string allocation.
        let bytes = [
            &context.decimal_separator,
            &context.grouping_separator,
            &context.positive_prefix,
            &context.positive_suffix,
            &context.negative_prefix,
            &context.negative_suffix,
        ]
        .iter()
        .fold(0usize, |total, text| total.saturating_add(text.len()));
        self.host.check_text(bytes)?;
        self.host.check_values(6)?;
        self.host.charge_work(bytes as u64)?;
        Ok(context.clone())
    }
}
