/// Date and time builtin handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DateFunction {
    /// DATE handler.
    Date,
    /// DATEDIF handler.
    DateDif,
    /// DATEVALUE handler.
    DateValue,
    /// DAY handler.
    Day,
    /// DAYS handler.
    Days,
    /// DAYS360 handler.
    Days360,
    /// EDATE handler.
    EDate,
    /// EOMONTH handler.
    EOMonth,
    /// HOUR handler.
    Hour,
    /// MINUTE handler.
    Minute,
    /// MONTH handler.
    Month,
    /// NETWORKDAYS handler.
    NetworkDays,
    /// NOW handler.
    Now,
    /// SECOND handler.
    Second,
    /// TIME handler.
    Time,
    /// TIMEVALUE handler.
    TimeValue,
    /// TODAY handler.
    Today,
    /// WEEKDAY handler.
    WeekDay,
    /// WEEKNUM handler.
    WeekNum,
    /// WORKDAY handler.
    WorkDay,
    /// WORKDAY.INTL handler.
    WorkDayIntl,
    /// YEAR handler.
    Year,
    /// YEARFRAC handler.
    YearFrac,
    /// ISO week number.
    IsoWeekNum,
    /// Custom weekend business-day count.
    NetworkDaysIntl,
}
