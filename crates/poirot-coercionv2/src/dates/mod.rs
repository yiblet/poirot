//! Calendar-independent date and time argument parsing plans.
/// Calendar primitives shared by the parsing plans and evaluator calendar algorithms.
pub mod calendar;
/// DATEVALUE parsing and explicit date-order resolution.
pub mod text;
/// TIMEVALUE normalization and arithmetic.
pub mod time;
/// Source formatter plan.
pub mod time_formatter;
/// Source date parser plan.
pub mod time_grammar;
/// Ordering of ambiguous slash-separated month/day fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder {
    /// Month precedes day.
    MonthDayYear,
    /// Day precedes month.
    DayMonthYear,
}

pub mod interval;
