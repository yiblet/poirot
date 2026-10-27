//! Typed date kernels with broadcast controls and fixed holiday collections.
use super::business_days::weekend_policy;
use super::components::{component, convert_numeric_outcome, month, week};
use super::day_count::{days360, yearfrac};
use super::text_coercion::{coerce_date_text, coerce_time_text};
use super::{business_days, components};
use crate::eval::adapters::elementwise::{
    Broadcast, Kernel, MemberContext, NumericParameter, NumericSignature, Optional, Variadic,
    WholeCollection, execute as execute_kernel,
};
use crate::eval::config::calendar::{calendar_date, max_serial, serial};
use crate::eval::functions::registry::{DateFunction, FunctionId};
use crate::eval::host::{ArgumentCoercion, CallHost, ContextReads, SelectionAccess};
use crate::eval::limits::{EvalResult, EvaluationFailure, WorkKind};
use crate::eval::values::{
    FunctionFailure, FunctionResult, OperandOrigin, VALUE_ERROR, Value, err,
};
use crate::model::CellError::Num;
use crate::model::CellValue::{Blank, Number};
use crate::model::{CellError, CellValue};
use poirot_functions::coercion::compatibility::coerce_date_integer;
use poirot_functions::coercion::dates::calendar::Date;
use poirot_functions::coercion::{
    Origin,
    collections::{HolidayPolicy, WeekendConsumer, coerce_holiday_member, coerce_weekend_mask},
    date_time::CalendarEpoch,
};

/// Resolve modern Excel components while retaining its fictitious February 29.
fn modern_date(serial: f64, system: crate::model::DateSystem) -> Result<Date, CellError> {
    let date = poirot_functions::format::CalendarPolicy {
        date_system: system,
        phantom_day: poirot_functions::format::PhantomDayPolicy::DisplayFebruary29,
    }
    .date(serial)
    .map_err(|_| Num)?;
    Ok(Date {
        y: date.year,
        m: i64::from(date.month),
        d: i64::from(date.day),
    })
}

/// Applies the selected numeric date function to each broadcast output member.
/// The workbook date system supplies the calendar interpretation for the independent date algorithms.
struct NumericDate {
    function: DateFunction,
    system: crate::model::DateSystem,
}

impl Kernel for NumericDate {
    type Signature = Variadic<Broadcast>;

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        args: &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use DateFunction::{
            Date, Day, Days, Days360, EDate, EOMonth, Hour, IsoWeekNum, Minute, Month, Second,
            Time, WeekDay, WeekNum, Year,
        };
        let (function, system) = (self.function, self.system);
        let parameters: &[NumericParameter] = match function {
            Date | Time => &[
                NumericParameter::required(),
                NumericParameter::required(),
                NumericParameter::required(),
            ],
            Days360 => &[
                NumericParameter::date_serial(),
                NumericParameter::date_serial(),
                NumericParameter::boolean_default(false),
            ],
            Days => &[
                NumericParameter::date_serial(),
                NumericParameter::date_serial(),
            ],
            EDate | EOMonth => &[
                NumericParameter::date_serial(),
                NumericParameter::required(),
            ],
            WeekDay | WeekNum => &[
                NumericParameter::date_serial(),
                NumericParameter::constant(1.).missing_as_blank(),
            ],
            _ => &[NumericParameter::date_serial()],
        };
        let numbers = args.numbers(context, parameters)?;
        let start = numbers.get(0)?;
        let result = match function {
            Date => components::excel_date(start, numbers.get(1)?, numbers.get(2)?, system)?,
            Time => convert_numeric_outcome(
                Time,
                coerce_date_integer(start.floor())?,
                coerce_date_integer(numbers.get(1)?.floor())?,
                coerce_date_integer(numbers.get(2)?.floor())?,
                system,
            )?,
            Days => start - numbers.get(1)?,
            Days360 => days360(
                modern_date(start, system)?,
                modern_date(numbers.get(1)?, system)?,
                numbers.get(2)? != 0.,
            ) as f64,
            EDate | EOMonth => month(
                function,
                start,
                modern_date(start, system)?,
                coerce_date_integer(numbers.get(1)?)?,
                system,
            )?,
            WeekDay => {
                components::excel_weekday(start, coerce_date_integer(numbers.get(1)?)?, system)?
            }
            WeekNum => {
                let d = modern_date(start, system)?;
                let mode = coerce_date_integer(numbers.get(1)?)?;
                if mode == 21 {
                    components::iso_week(d)
                } else {
                    week(function, start, d, mode, system)?
                }
            }
            IsoWeekNum => {
                poirot_functions::dates::iso_week(start.trunc() as i64, epoch_for(system))?
            }
            Year | Month | Day | Hour | Minute | Second => {
                component(function, start, modern_date(start, system)?, system)?
            }
            _ => return Err(EvaluationFailure::StalePlan.into()),
        };
        Ok(Number(result))
    }
}

/// NOW or TODAY kernel that reads the captured clock without any source arguments.
struct ClockDate(DateFunction);

impl Kernel for ClockDate {
    type Signature = ();

    fn apply(&self, context: &mut MemberContext<'_, '_, '_>, _: &()) -> FunctionResult<CellValue> {
        let value = ContextReads::new(context.host).clock_serial()?;
        Ok(Number(if self.0 == DateFunction::Today {
            value.floor()
        } else {
            value
        }))
    }
}

/// DATEVALUE or TIMEVALUE kernel for broadcast text operands.
/// Parsing requests localization and clock fields only when the input needs them.
struct TextDate {
    function: DateFunction,
    system: crate::model::DateSystem,
}

impl Kernel for TextDate {
    type Signature = (Broadcast,);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (source,): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let operand = source.read(context)?;
        if operand.missing {
            return Err(VALUE_ERROR.into());
        }
        let text = ArgumentCoercion::new(context.host).coerce_scalar_text(&operand.value)?;
        Ok(if self.function == DateFunction::DateValue {
            Number(serial(
                coerce_date_text(context.host, &text, self.system)?,
                self.system,
            )?)
        } else if text.is_empty() {
            Blank
        } else {
            Number(coerce_time_text(context.host, &text)?)
        })
    }
}

/// YEARFRAC or DATEDIF kernel over broadcast endpoints.
/// It keeps numeric basis selection and textual interval selection on their distinct coercion paths.
struct IntervalDate {
    function: DateFunction,
    system: crate::model::DateSystem,
}

impl Kernel for IntervalDate {
    type Signature = (Broadcast, Broadcast, Optional<Broadcast>);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (start, end, selector): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        if self.function == DateFunction::YearFrac {
            let basis = NumericSignature::new(&[
                NumericParameter::constant(0.).integer_choices(&[0, 1, 2, 3, 4])
            ])
            .numbers_roles(context, [selector.0])?
            .get(0)?;
            let basis = coerce_date_integer(basis.floor())?;
            if !(0..=4).contains(&basis) {
                return Err(Num.into());
            }
            let dates = NumericSignature::new(&[NumericParameter::date_serial(); 2])
                .numbers_roles(context, [Some(*start), Some(*end)])?;
            let (start, end) = (dates.get(0)?, dates.get(1)?);
            return Ok(Number(yearfrac(
                start,
                end,
                calendar_date(start, self.system)?,
                calendar_date(end, self.system)?,
                basis,
            )?));
        }
        let dates = NumericSignature::new(&[NumericParameter::date_serial(); 2])
            .numbers_roles(context, [Some(*start), Some(*end)])?;
        let unit = selector.0.as_ref().ok_or(VALUE_ERROR)?.read(context)?;
        let unit = ArgumentCoercion::new(context.host).coerce_scalar_text(&unit.value)?;
        let epoch = epoch_for(self.system);
        use poirot_functions::coercion::dates::interval::{
            coerce_datedif_day, coerce_datedif_unit,
        };
        Ok(Number(poirot_functions::dates::datedif(
            coerce_datedif_day(dates.get(0)?, epoch)?,
            coerce_datedif_day(dates.get(1)?, epoch)?,
            coerce_datedif_unit(&unit)?,
            epoch,
        )?))
    }
}

/// Convert a workbook calendar to the independent date algorithm's epoch.
fn epoch_for(system: crate::model::DateSystem) -> CalendarEpoch {
    match system {
        crate::model::DateSystem::Excel1900 => CalendarEpoch::Excel1900,
        crate::model::DateSystem::Excel1904 => CalendarEpoch::Excel1904,
    }
}

/// Business-day kernel that broadcasts endpoints and weekend options over a shared holiday list.
/// The list is read and converted once when first needed, then reused for later output members.
struct BusinessDate {
    function: DateFunction,
    system: crate::model::DateSystem,
    holidays: std::cell::RefCell<Option<Vec<i64>>>,
}

impl BusinessDate {
    /// Prepare the fixed holiday source independently of broadcast source ordinals.
    fn holidays(
        &self,
        host: &mut CallHost<'_, '_>,
        collection: Option<&WholeCollection>,
    ) -> FunctionResult<()> {
        if self
            .holidays
            .try_borrow()
            .map_err(|_| EvaluationFailure::StalePlan)?
            .is_some()
        {
            return Ok(());
        }
        let mut holidays = Vec::new();
        let Some(collection) = collection else {
            *self
                .holidays
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)? = Some(holidays);
            return Ok(());
        };
        let count = collection.len(host)?;
        host.check_values(count)?;
        holidays
            .try_reserve_exact(count)
            .map_err(|_| host.allocation_failure())?;
        let policy = if matches!(
            self.function,
            DateFunction::NetworkDays | DateFunction::NetworkDaysIntl
        ) {
            HolidayPolicy::NetworkDays
        } else {
            HolidayPolicy::WorkDay
        };
        let mut holidays = crate::eval::adapters::elementwise::collections::fold(
            host,
            [*collection],
            holidays,
            |host, holidays, member| {
                // A supplied omitted holiday is numeric zero; an absent holiday
                // collection contributes nothing and real blank members are ignored.
                let value = if member.missing {
                    Number(0.)
                } else {
                    member.value
                };
                let origin = match member.origin {
                    OperandOrigin::Direct => Origin::Direct,
                    OperandOrigin::ArrayElement => Origin::Array,
                    OperandOrigin::ReferencedCell => Origin::Reference,
                };
                if let Some(day) = coerce_holiday_member((&value).into(), origin, policy, || {
                    ArgumentCoercion::new(host).coerce_calendar_date(&value)
                })? {
                    if holidays.len() == holidays.capacity() {
                        holidays
                            .try_reserve(1)
                            .map_err(|_| host.allocation_failure())?;
                    }
                    // SAFETY: the capacity check above reserves a slot whenever none remains.
                    holidays.push(day.trunc() as i64);
                }
                Ok(())
            },
        )?;
        // SAFETY: i64 comparison is total and sort_unstable allocates no auxiliary storage.
        holidays.sort_unstable();
        holidays.dedup();
        *self
            .holidays
            .try_borrow_mut()
            .map_err(|_| EvaluationFailure::StalePlan)? = Some(holidays);
        Ok(())
    }
}

impl Kernel for BusinessDate {
    type Signature = (
        Broadcast,
        Broadcast,
        Optional<Broadcast>,
        Optional<WholeCollection>,
    );

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (first, second, weekend, collection): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        let network = matches!(
            self.function,
            DateFunction::NetworkDays | DateFunction::NetworkDaysIntl
        );
        let numbers = NumericSignature::new(&[
            NumericParameter::date_serial(),
            if network {
                NumericParameter::date_serial()
            } else {
                NumericParameter::required()
            },
        ])
        .numbers_roles(context, [Some(*first), Some(*second)])?;
        let start = numbers.get(0)?.trunc() as i64;
        let weekend = if let Some(role) = &weekend.0 {
            let member = role.read(context)?;
            if member.missing {
                weekend_policy(1)?
            } else if let Some(text) = member.value.text_ref() {
                let text = ArgumentCoercion::new(context.host).project_text(text)?;
                coerce_weekend_mask(
                    &text,
                    if network {
                        WeekendConsumer::NetworkDays
                    } else {
                        WeekendConsumer::WorkDay
                    },
                )?
            } else {
                weekend_policy(coerce_date_integer(
                    SelectionAccess::new(context.host).number(&member.value)?,
                )?)?
            }
        } else {
            weekend_policy(1)?
        };
        self.holidays(context.host, collection.0.as_ref())?;
        let holidays = self
            .holidays
            .try_borrow()
            .map_err(|_| EvaluationFailure::StalePlan)?;
        let holidays = holidays.as_ref().ok_or(EvaluationFailure::StalePlan)?;
        if network {
            return Ok(Number(poirot_functions::dates::network_days(
                start,
                numbers.get(1)?.trunc() as i64,
                epoch_for(self.system),
                &weekend,
                holidays,
            )?));
        }
        let mut count = coerce_date_integer(numbers.get(1)?)?;
        let direction = if count < 0 { -1 } else { 1 };
        count = count.checked_abs().ok_or(Num)?;
        if count > max_serial(self.system) {
            return Err(Num.into());
        }
        let mut day = start;
        while count > 0 {
            context.host.charge_kind(WorkKind::CalendarStep, 1)?;
            (day, count) = business_days::advance_serial(
                day,
                count,
                direction,
                self.system,
                &weekend,
                holidays,
            )?;
        }
        Ok(Number(day as f64))
    }
}

/// Adapts WORKDAY and NETWORKDAYS to the international business-day kernel.
/// It places the optional holiday collection in the shared signature while leaving the weekend option absent.
struct StandardBusiness(BusinessDate);

impl Kernel for StandardBusiness {
    type Signature = (Broadcast, Broadcast, Optional<WholeCollection>);

    fn apply(
        &self,
        context: &mut MemberContext<'_, '_, '_>,
        (first, second, collection): &Self::Signature,
    ) -> FunctionResult<CellValue> {
        use crate::eval::adapters::elementwise::Signature;
        let mut args = <BusinessDate as Kernel>::Signature::bind(0);
        args.0 = *first;
        args.1 = *second;
        args.3.0 = collection.0;
        self.0.apply(context, &args)
    }
}

/// Execute the selected typed date signature with shared failure handling.
pub(super) fn execute(host: &mut CallHost<'_, '_>, function: DateFunction) -> EvalResult<Value> {
    if !FunctionId::Date(function)
        .descriptor()
        .arity
        .accepts(host.len())
    {
        return Ok(err(VALUE_ERROR));
    }
    let system = if matches!(
        function,
        DateFunction::Time | DateFunction::TimeValue | DateFunction::Now | DateFunction::Today
    ) {
        crate::model::DateSystem::Excel1900
    } else {
        ContextReads::new(host).selected_date_system()
    };
    let at = host.target().address;
    let result = match function {
        DateFunction::Now | DateFunction::Today => execute_kernel(host, at, ClockDate(function)),
        DateFunction::DateValue | DateFunction::TimeValue => {
            execute_kernel(host, at, TextDate { function, system })
        }
        DateFunction::DateDif | DateFunction::YearFrac => {
            execute_kernel(host, at, IntervalDate { function, system })
        }
        DateFunction::NetworkDays | DateFunction::WorkDay => execute_kernel(
            host,
            at,
            StandardBusiness(BusinessDate {
                function,
                system,
                holidays: std::cell::RefCell::new(None),
            }),
        ),
        DateFunction::NetworkDaysIntl | DateFunction::WorkDayIntl => execute_kernel(
            host,
            at,
            BusinessDate {
                function,
                system,
                holidays: std::cell::RefCell::new(None),
            },
        ),
        _ => execute_kernel(host, at, NumericDate { function, system }),
    };
    match result {
        Ok(value) => Ok(value),
        Err(FunctionFailure::Spreadsheet(error)) => Ok(err(error)),
        Err(FunctionFailure::Operational(error)) => Err(error),
    }
}
