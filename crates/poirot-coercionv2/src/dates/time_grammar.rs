//! TIMEVALUE argument plan over shared syntax and strict calendar resolution.
use super::calendar::{Date, month_days};
use poirot_functions_core::{CellError, CellError::Value as VALUE_ERROR};

/// Inspected TIMEVALUE date fields before optional default-year resolution.
#[derive(Clone, Copy)]
pub struct DatePlan<'a>(poirot_parsing::date::TimeValueDate<'a>);
fn number(s: &str) -> i64 {
    s.parse().unwrap_or(-1)
}
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
fn month(s: &str) -> i64 {
    if s.bytes().all(|b| b.is_ascii_digit()) {
        return number(s);
    }
    MONTHS
        .iter()
        .position(|m| m.get(..s.len()).is_some_and(|p| p.eq_ignore_ascii_case(s)))
        .map_or(-1, |n| n as i64 + 1)
}
impl<'a> DatePlan<'a> {
    /// Inspect the independent source grammar before requesting calendar context.
    pub fn inspect(input: &'a str) -> Result<Self, CellError> {
        poirot_parsing::date::TimeValueDate::inspect(input)
            .map(Self)
            .map_err(|_| VALUE_ERROR)
    }
    /// Whether the host must supply a captured current year.
    pub fn needs_year(self) -> bool {
        self.0.year.is_none()
    }
    /// Resolve inspected fields with an explicit default year and Gregorian validation.
    pub fn resolve(self, year: i64) -> Result<Date, CellError> {
        let date = Date {
            y: self.0.year.unwrap_or(year),
            m: month(self.0.month),
            d: self.0.day,
        };
        if date.y < 0
            || date.y > 9999
            || !(1..=12).contains(&date.m)
            || date.d < 1
            || date.d > month_days(date.y, date.m)
        {
            return Err(VALUE_ERROR);
        }
        Ok(date)
    }
}

#[cfg(test)]
#[path = "time_grammar_tests.rs"]
mod tests;
