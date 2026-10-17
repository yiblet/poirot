//! Raw date serial calculations, separate from display formatting.
//! Calendar arithmetic uses a proleptic Gregorian calendar and explicitly models
//! Excel's 1900 leap-day slot. No machine-local timezone or locale is consulted.
use crate::*;
#[cfg(test)]
use poirot_model::DateSystem;
mod business_days;
mod calendar;
mod components;
mod day_count;
use business_days::{WorkdayPlan, WorkdayState, advance, network, weekend_policy};
use calendar::{
    Date, calendar_date, civil, coercev2_date_integer, date, epoch, max_serial, serial, valid,
};
use components::{coerce_numeric_outcome, component, month, week};
use day_count::{days360, yearfrac};
use poirot_coercionv2::dates::text;
use poirot_coercionv2::dates::time;
use poirot_coercionv2::dates::time_formatter;
use poirot_coercionv2::dates::time_grammar;
use text::DateTextPlan;

impl Session<'_> {
    fn clock_serial(&mut self) -> Result<f64, CellError> {
        self.read(DependencyRead::ContextField(run::ContextRead::AsOf));
        self.read(DependencyRead::ContextField(run::ContextRead::UtcOffset));
        let o = self.context;
        if !(-86400..=86400).contains(&o.localization.utc_offset_seconds) {
            return Err(Num);
        }
        let sec =
            crate::clock::unix_seconds(o.as_of_time) + f64::from(o.localization.utc_offset_seconds);
        let days = (sec / 86400.).floor();
        if !(-25568. ..2_932_897.).contains(&days) {
            return Err(Num);
        }
        let d = civil(days as i64);
        self.read(DependencyRead::ContextField(run::ContextRead::DateSystem));
        valid(
            serial(d, self.workbook.date_system)? + sec.rem_euclid(86400.) / 86400.,
            self.workbook.date_system,
        )
    }
    fn coercev2_date_text(&mut self, t: &str) -> Result<Date, CellError> {
        let plan = DateTextPlan::inspect(t)?;
        if plan.reads_date_order {
            self.read(DependencyRead::ContextField(run::ContextRead::DateOrder));
        }
        let order = match self.context.localization.date_order {
            DateOrder::MonthDayYear => poirot_coercionv2::dates::DateOrder::MonthDayYear,
            DateOrder::DayMonthYear => poirot_coercionv2::dates::DateOrder::DayMonthYear,
        };
        plan.coercev2_resolve(order, || {
            Ok(date(self.clock_serial()?, self.workbook.date_system)?.y)
        })
    }
    fn time_default_year(&mut self) -> Result<i64, CellError> {
        self.read(DependencyRead::ContextField(run::ContextRead::AsOf));
        self.read(DependencyRead::ContextField(run::ContextRead::UtcOffset));
        let seconds = crate::clock::unix_seconds(self.context.as_of_time);
        let offset = self.context.localization.utc_offset_seconds;
        if !(-86400..=86400).contains(&offset) {
            return Err(Num);
        }
        let days = ((seconds + f64::from(offset)) / 86400.).floor();
        if !(-25568. ..2_932_897.).contains(&days) {
            return Err(Num);
        }
        Ok(civil(days as i64).y)
    }
    fn time_value_stage(
        &mut self,
        original: &str,
        normalized: &str,
    ) -> Result<Option<f64>, CellError> {
        let Ok(util) =
            time_formatter::Plan::inspect(time_formatter::SourceProfile::EnglishUtc, normalized)
        else {
            return Ok(None);
        };
        let Ok(parser) = time_grammar::DatePlan::inspect(original) else {
            return Ok(None);
        };
        let year = if util.needs_year() || parser.needs_year() {
            self.time_default_year()?
        } else {
            2000
        };
        let (Ok(util), Ok(date)) = (util.resolve(year), parser.resolve(year)) else {
            return Ok(None);
        };
        let addition = util.date.map_or(0., time::source_day);
        Ok(Some(time::fraction(
            addition,
            util.seconds,
            time::source_day(date),
        )))
    }
    fn coercev2_time_text(&mut self, input: &str) -> FunctionResult<f64> {
        self.budget.text(input.len())?;
        let normalized = time::normalize(input);
        if let Some(value) = self.time_value_stage(input, &normalized)? {
            return Ok(value);
        }
        // Explicit second source stage, including the same four optional
        // formatter sections and DateParser's ORIGINAL dummy-prefixed input.
        let dummy_bytes = input.len().checked_add("1/01/2000 ".len()).ok_or_else(|| {
            EvaluationFailure::ResourceLimit {
                capability: CapabilityId::DateParsing,
                source: None,
                detail: "TIMEVALUE dummy prefix length overflow".into(),
            }
        })?;
        self.budget.text(dummy_bytes)?;
        let mut dummy = String::with_capacity(dummy_bytes);
        dummy.push_str("1/01/2000 ");
        dummy.push_str(input);
        self.budget.text(dummy.len())?;
        let normalized_dummy = time::normalize(&dummy);
        if let Some(value) = self.time_value_stage(&dummy, &normalized_dummy)? {
            return Ok(value);
        }
        let plan = time_grammar::DatePlan::inspect(input)?;
        let year = if plan.needs_year() {
            self.time_default_year()?
        } else {
            2000
        };
        Ok(plan.resolve(year).map(|_| 0.)?)
    }
    fn coercev2_date_argument(&mut self, v: &CellValue) -> FunctionResult<f64> {
        let n = if let Some(text) = v.text_ref() {
            let text = checked_text_view(text, &mut self.budget)?;
            match poirot_coercionv2::compatibility::coercev2_date_numeric_text(&text) {
                poirot_coercionv2::compatibility::NumericTextDecision::Number(n) => n?,
                poirot_coercionv2::compatibility::NumericTextDecision::DateTime => {
                    serial(self.coercev2_date_text(&text)?, self.workbook.date_system)?
                }
            }
        } else {
            self.coercev2_scalar_number(v)?
        };
        Ok(valid(n, self.workbook.date_system)?)
    }
    fn coercev2_requested_date_serial(
        &mut self,
        value: &CellValue,
        policy: poirot_coercionv2::compatibility::DateSerialPolicy,
    ) -> FunctionResult<f64> {
        use poirot_coercionv2::compatibility::{
            DeferredNumber, NumericTextDecision, coercev2_date_serial,
        };
        coercev2_date_serial(value.into(), policy, |request| match request {
            DeferredNumber::Missing => Err(VALUE_ERROR.into()),
            DeferredNumber::Text(text) => {
                let text = checked_text_view(text, &mut self.budget)?;
                match self.coercev2_formatted_text(&text)? {
                    NumericTextDecision::Number(n) => n.map_err(Into::into),
                    NumericTextDecision::DateTime => self.coercev2_numeric_date_time(&text),
                }
            }
        })
    }
    fn coercev2_datedif_argument(&mut self, value: &CellValue) -> FunctionResult<f64> {
        use poirot_coercionv2::compatibility::{
            DateSerialPolicy, DeferredNumber, NumericTextDecision, coercev2_date_serial,
        };
        coercev2_date_serial(
            value.into(),
            DateSerialPolicy::Calendar,
            |request| match request {
                DeferredNumber::Missing => Err(VALUE_ERROR.into()),
                DeferredNumber::Text(text) => {
                    let text = checked_text_view(text, &mut self.budget)?;
                    match poirot_coercionv2::compatibility::coercev2_date_numeric_text(&text) {
                        NumericTextDecision::Number(n) => n.map_err(Into::into),
                        NumericTextDecision::DateTime => Ok(serial(
                            self.coercev2_date_text(&text)?,
                            self.workbook.date_system,
                        )?),
                    }
                }
            },
        )
    }
    fn coercev2_yearfrac_date_argument(&mut self, v: &CellValue) -> FunctionResult<f64> {
        // Numeric text bypasses checkDate as in the pinned source.
        if let Some(text) = v.text_ref() {
            let text = checked_text_view(text, &mut self.budget)?;
            if let Ok(n) = poirot_coercionv2::compatibility::coercev2_arithmetic_text(&text) {
                coercev2_date_integer(n.floor())?;
                return Ok(n);
            }
        }
        self.coercev2_date_argument(v)
    }
    fn holiday_values(&mut self, value: Value) -> EvalResult<Vec<CellValue>> {
        match value {
            Value::Missing => {
                self.budget.allocation(1)?;
                Ok(vec![coerce_legacy_missing_blank()])
            }
            Value::Scalar(value) | Value::SelectedScalar(value, _) => {
                self.budget.allocation(1)?;
                Ok(vec![value])
            }
            Value::Array(rows) => {
                self.budget.allocation(rows.iter().map(Vec::len).sum())?;
                Ok(rows.into_iter().flatten().collect())
            }
            Value::SignedRefs(areas) => {
                let total = areas.iter().map(|a| a.size()).sum();
                self.budget.allocation_u64(total)?;
                let mut output = Vec::with_capacity(total as usize);
                for area in areas {
                    for row in 0..area.height() {
                        for col in 0..area.width() {
                            output.push(self.signed_read(
                                area.read(row, col).ok_or(EvaluationFailure::StalePlan)?,
                            )?);
                        }
                    }
                }
                Ok(output)
            }
            Value::Refs(areas) | Value::SheetSpan(areas) => {
                let total = areas.iter().map(|area| area.size()).sum();
                self.budget.allocation_u64(total)?;
                let mut output = Vec::with_capacity(total as usize);
                for area in areas {
                    for row in area.r0..=area.r1 {
                        for col in area.c0..=area.c1 {
                            output.push(self.cell(area.sheet, CellAddress { row, col })?);
                        }
                    }
                }
                Ok(output)
            }
        }
    }
    /// Execute date and time builtins while recording consumed calendar and clock inputs.
    pub(crate) fn date_function(
        &mut self,
        s: usize,
        at: CellAddress,
        name: DateFunction,
        args: ArgumentsRef<'_>,
    ) -> EvalResult<Value> {
        match name {
            DateFunction::Date
            | DateFunction::DateDif
            | DateFunction::DateValue
            | DateFunction::Day
            | DateFunction::Days
            | DateFunction::Days360
            | DateFunction::EDate
            | DateFunction::EOMonth
            | DateFunction::Hour
            | DateFunction::Minute
            | DateFunction::Month
            | DateFunction::NetworkDays
            | DateFunction::Now
            | DateFunction::Second
            | DateFunction::Time
            | DateFunction::TimeValue
            | DateFunction::Today
            | DateFunction::WeekDay
            | DateFunction::WeekNum
            | DateFunction::WorkDay
            | DateFunction::WorkDayIntl
            | DateFunction::Year
            | DateFunction::YearFrac => {
                let result = self.date_result(s, at, name, args);
                function_value(result)
            }
        }
    }
    fn date_result(
        &mut self,
        s: usize,
        at: CellAddress,
        name: DateFunction,
        args: ArgumentsRef<'_>,
    ) -> FunctionResult<CellValue> {
        let n = args.len();
        let system = self.workbook.date_system;
        let excel = self.builtin_policy == BuiltinSemanticsPolicy::ExcelDocumentation;
        if !FunctionId::Date(name).descriptor().arity.accepts(n) {
            return Err(VALUE_ERROR.into());
        }
        // TIME and standalone TIMEVALUE do not consume the workbook epoch.
        // Other date handlers retain a conservative epoch guard.
        if !matches!(
            name,
            DateFunction::Time | DateFunction::TimeValue | DateFunction::Now | DateFunction::Today
        ) {
            self.read(DependencyRead::ContextField(run::ContextRead::DateSystem));
        }
        if name == DateFunction::Now || name == DateFunction::Today {
            let t = self.clock_serial()?;
            return Ok(Number(if name == DateFunction::Today {
                t.floor()
            } else {
                t
            }));
        }
        let yearfrac_basis = if name == DateFunction::YearFrac && n == 3 {
            let c = self.argument(s, at, args, 2)?;
            Some(coercev2_date_integer(
                self.coercev2_scalar_number(&c)?.floor(),
            )?)
        } else {
            None
        };
        let a = self.argument(s, at, args, 0)?;
        if name == DateFunction::DateDif {
            let start = self.coercev2_datedif_argument(&a)?;
            let b = self.argument(s, at, args, 1)?;
            let end = self.coercev2_datedif_argument(&b)?;
            let c = self.argument(s, at, args, 2)?;
            let unit = coercev2_formula_text(&c, &mut self.budget)?;
            use poirot_coercionv2::{
                date_time::CalendarEpoch,
                dates::interval::{coercev2_datedif_day, coercev2_datedif_unit},
            };
            let epoch = match system {
                poirot_model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
                poirot_model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
            };
            let start = coercev2_datedif_day(start, epoch)?;
            let end = coercev2_datedif_day(end, epoch)?;
            let unit = coercev2_datedif_unit(&unit)?;
            return Ok(Number(poirot_functions_dates::datedif(
                start, end, unit, epoch,
            )?));
        }
        if name == DateFunction::DateValue || name == DateFunction::TimeValue {
            let t = coercev2_formula_text(&a, &mut self.budget)?;
            if name == DateFunction::TimeValue && t.is_empty() {
                return Ok(Blank);
            }
            return Ok(Number(if name == DateFunction::DateValue {
                serial(self.coercev2_date_text(&t)?, system)?
            } else {
                self.coercev2_time_text(&t)?
            }));
        }
        if name == DateFunction::Date || name == DateFunction::Time {
            let x = coercev2_date_integer(if name == DateFunction::Time {
                self.coercev2_scalar_number(&a)?.floor()
            } else {
                self.coercev2_scalar_number(&a)?
            })?;
            let b = self.argument(s, at, args, 1)?;
            let y = if name == DateFunction::Time {
                coercev2_date_integer(self.coercev2_scalar_number(&b)?.floor())?
            } else if excel {
                coercev2_date_integer(self.coercev2_scalar_number(&b)?)?
            } else {
                coercev2_date_integer(self.coercev2_scalar_number(&b)? - 1.)? + 1
            };
            let c = self.argument(s, at, args, 2)?;
            let z = coercev2_date_integer(if name == DateFunction::Time {
                self.coercev2_scalar_number(&c)?.floor()
            } else {
                self.coercev2_scalar_number(&c)?
            })?;
            return Ok(Number(if excel && name == DateFunction::Date {
                components::excel_date(x, y, z, system)?
            } else {
                // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                coerce_numeric_outcome(name, x, y, z, system)?
            }));
        }
        let edate_offset = if name == DateFunction::EDate && !excel {
            use poirot_coercionv2::compatibility::DateSerialPolicy::PinnedNumberOrBlank;
            self.coercev2_requested_date_serial(&a, PinnedNumberOrBlank)?;
            let b = self.argument(s, at, args, 1)?;
            Some(self.coercev2_requested_date_serial(&b, PinnedNumberOrBlank)?)
        } else {
            None
        };
        let textual_date = matches!(
            name,
            DateFunction::Days
                | DateFunction::Days360
                | DateFunction::NetworkDays
                | DateFunction::WorkDay
                | DateFunction::WorkDayIntl
                | DateFunction::YearFrac
        );
        let start = if excel && matches!(name, DateFunction::EDate | DateFunction::WeekDay) {
            valid(
                self.coercev2_requested_date_serial(
                    &a,
                    poirot_coercionv2::compatibility::DateSerialPolicy::Calendar,
                )?,
                system,
            )?
        } else if name == DateFunction::YearFrac {
            self.coercev2_yearfrac_date_argument(&a)?
        } else if textual_date {
            self.coercev2_date_argument(&a)?
        } else {
            valid(
                if name == DateFunction::WeekNum {
                    self.coercev2_scalar_number(&a)
                        .map_err(|e| remap_semantic(e, VALUE_ERROR))?
                } else {
                    self.coercev2_scalar_number(&a)?
                },
                system,
            )?
        };
        if excel && name == DateFunction::Days {
            let b = self.argument(s, at, args, 1)?;
            return Ok(Number(start - self.coercev2_date_argument(&b)?));
        }
        let d = if name == DateFunction::YearFrac {
            calendar_date(start, system)?
        } else {
            date(start, system)?
        };
        if matches!(
            name,
            DateFunction::Year
                | DateFunction::Month
                | DateFunction::Day
                | DateFunction::Hour
                | DateFunction::Minute
                | DateFunction::Second
        ) {
            return Ok(Number(component(name, start, d, system)?));
        }
        let b = if let Some(offset) = edate_offset {
            Number(offset)
        } else if n >= 2 {
            self.argument(s, at, args, 1)?
        } else {
            Number(1.)
        };
        if name == DateFunction::WeekDay || name == DateFunction::WeekNum {
            let mode = if excel && name == DateFunction::WeekDay {
                use poirot_coercionv2::compatibility::{DeferredNumber, coercev2_weekday_option};
                coercev2_weekday_option((&b).into(), |request| match request {
                    DeferredNumber::Text(_) => self.coercev2_scalar_number(&b),
                    DeferredNumber::Missing => Err(VALUE_ERROR.into()),
                })?
            } else {
                coercev2_date_integer(
                    self.coercev2_scalar_number(&b)
                        .map_err(|e| remap_semantic(e, Num))?,
                )?
            };
            if excel && name == DateFunction::WeekDay {
                return Ok(Number(components::excel_weekday(start, mode, system)?));
            }
            if excel && name == DateFunction::WeekNum && mode == 21 {
                return Ok(Number(components::iso_week(d)));
            }
            return Ok(Number(week(name, start, d, mode, system)?));
        }
        if name == DateFunction::EDate || name == DateFunction::EOMonth {
            // EDATE's POI operand contract accepts numbers/blank but not text/bools.
            if name == DateFunction::EDate && !excel && !matches!(a, Number(_) | Blank) {
                return Err(VALUE_ERROR.into());
            }
            let months = coercev2_date_integer(self.coercev2_scalar_number(&b)?)?;
            return Ok(Number(month(name, start, d, months, system)?));
        }
        if name == DateFunction::Days {
            return Ok(Number(
                (epoch(d) - epoch(date(self.coercev2_date_argument(&b)?, system)?)) as f64,
            ));
        }
        if name == DateFunction::Days360 || name == DateFunction::YearFrac {
            let end = if name == DateFunction::YearFrac {
                self.coercev2_yearfrac_date_argument(&b)?
            } else {
                self.coercev2_date_argument(&b)?
            };
            let e = if name == DateFunction::YearFrac {
                calendar_date(end, system)?
            } else {
                date(end, system)?
            };
            let c = if name == DateFunction::Days360 && n == 3 {
                self.argument(s, at, args, 2)?
            } else {
                Number(0.)
            };
            if name == DateFunction::Days360 {
                return Ok(Number(days360(
                    d,
                    e,
                    core_projection::coercev2_operand_boolean(&c, self.numeric_policy)?,
                ) as f64));
            }
            let basis = yearfrac_basis.unwrap_or(0);
            return Ok(Number(yearfrac(start, end, d, e, basis)?));
        }
        let intl = name == DateFunction::WorkDayIntl;
        let mut modern_mask = None;
        let weekend_mode = if intl && n >= 3 {
            let c = self.argument(s, at, args, 2)?;
            if excel && let Some(text) = c.text_ref() {
                let text = checked_text_view(text, &mut self.budget)?;
                modern_mask = Some(poirot_coercionv2::collections::coercev2_weekend_mask(
                    &text,
                    poirot_coercionv2::collections::WeekendConsumer::WorkDay,
                )?);
            }
            // Numeric text, including leading zeroes, is a POI weekend code.
            if modern_mask.is_some() || matches!(c, Blank) {
                1
            } else {
                coercev2_date_integer(self.coercev2_scalar_number(&c)?)?
            }
        } else {
            1
        };
        let weekends = modern_mask.unwrap_or(weekend_policy(weekend_mode)?);
        let hi = if intl { 3 } else { 2 };
        let mut holidays = Vec::new();
        if let Some(arg) = args.get(hi) {
            let v = self.expr(s, at, arg)?;
            use poirot_coercionv2::{
                Origin,
                collections::{HolidayPolicy, coercev2_holiday_member},
            };
            let omitted = arg.is_missing() || matches!(v, Value::Missing);
            let origin = match &v {
                Value::Scalar(_) | Value::Missing => Origin::Direct,
                Value::SelectedScalar(_, origin) => match origin {
                    host::OperandOrigin::Direct => Origin::Direct,
                    host::OperandOrigin::ReferencedCell => Origin::Reference,
                    host::OperandOrigin::ArrayElement => Origin::Array,
                },
                Value::Array(_) => Origin::Array,
                _ => Origin::Reference,
            };
            let policy = if !excel {
                HolidayPolicy::PinnedPoi
            } else if name == DateFunction::NetworkDays {
                HolidayPolicy::NetworkDays
            } else {
                HolidayPolicy::WorkDay
            };
            for v in self.holiday_values(v)? {
                self.budget
                    .check_allocation(holidays.len().saturating_add(1))?;
                self.budget.charge(WorkKind::Materialization, 1)?;
                let member = if omitted {
                    poirot_coercionv2::compatibility::ScalarRef::Missing
                } else {
                    (&v).into()
                };
                if let Some(holiday) = coercev2_holiday_member(member, origin, policy, || {
                    self.coercev2_date_argument(&v)
                })? {
                    holidays.push(holiday);
                }
            }
        }
        self.budget.allocation(holidays.len())?;
        if excel {
            self.budget.allocation(holidays.len())?;
            let mut serial_holidays: Vec<_> = holidays.iter().map(|h| h.trunc() as i64).collect();
            serial_holidays.sort_unstable();
            serial_holidays.dedup();
            if name == DateFunction::NetworkDays {
                let end = self.coercev2_date_argument(&b)?;
                return Ok(Number(business_days::network_serial(
                    start.trunc() as i64,
                    end.trunc() as i64,
                    system,
                    &weekends,
                    &serial_holidays,
                )?));
            }
            let mut count = coercev2_date_integer(self.coercev2_scalar_number(&b)?)?;
            let direction = if count < 0 { -1 } else { 1 };
            count = count.checked_abs().ok_or(Num)?;
            if count > max_serial(system) {
                return Err(Num.into());
            }
            let mut day = start.trunc() as i64;
            while count > 0 {
                self.budget.charge(WorkKind::CalendarStep, 1)?;
                (day, count) = business_days::advance_serial(
                    day,
                    count,
                    direction,
                    system,
                    &weekends,
                    &serial_holidays,
                )?;
            }
            return Ok(Number(day as f64));
        }
        let holiday_set: HashSet<_> = holidays
            .iter()
            .map(|h| h.round() as i64 - start.fract().round() as i64)
            .collect();
        if name == DateFunction::NetworkDays {
            let raw_end = self.coercev2_date_argument(&b)?;
            if start > raw_end {
                return Err(Name.into());
            }
            return Ok(Number(network(
                start, raw_end, system, &weekends, &holidays,
            )));
        }
        let raw_count = self.coercev2_scalar_number(&b)?;
        let mut count = coercev2_date_integer(if excel { raw_count } else { raw_count.floor() })?;
        let direction = if count < 0 { -1 } else { 1 };
        count = count.abs();
        if count > max_serial(system) {
            return Err(Num.into());
        }
        let mut calendar_day = epoch(d);
        let mut day = serial(d, system)?.floor() as i64;
        let first_calendar_day = epoch(date(0., system)?);
        let last_calendar_day = epoch(date(max_serial(system) as f64, system)?);
        let work_per_week = weekends.iter().filter(|b| !**b).count() as i64;
        self.budget.allocation(holiday_set.len())?;
        let mut sorted_holidays: Vec<_> = holiday_set.into_iter().collect();
        sorted_holidays.sort_unstable();
        let plan = WorkdayPlan {
            system,
            direction,
            first_calendar_day,
            last_calendar_day,
            work_per_week,
            weekends: &weekends,
            sorted_holidays: &sorted_holidays,
        };
        while count > 0 {
            self.budget.charge(WorkKind::CalendarStep, 1)?;
            let state = advance(
                WorkdayState {
                    day,
                    calendar_day,
                    count,
                },
                &plan,
            )?;
            day = state.day;
            calendar_day = state.calendar_day;
            count = state.count;
        }
        Ok(Number(valid(
            serial(civil(calendar_day), system)? + start.fract(),
            system,
        )?))
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod time_tests;

#[cfg(test)]
mod time_dispatch_tests;

#[cfg(test)]
#[path = "execution/numeric_boolean_tests.rs"]
mod numeric_boolean_tests;
