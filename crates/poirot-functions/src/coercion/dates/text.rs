use super::DateOrder;
use super::calendar::{Date, month_days};
use crate::scalar::{CellError, CellError::Value as VALUE_ERROR};

fn parse_month(s: &str) -> Option<i64> {
    if let Ok(n) = s.parse() {
        return Some(n);
    }
    const MONTHS: [&str; 12] = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let s = s.to_ascii_lowercase();
    (!s.is_empty())
        .then(|| MONTHS.iter().position(|m| m.starts_with(&s)))
        .flatten()
        .map(|i| i as i64 + 1)
}
fn date_parts(input: &str) -> Result<Vec<&str>, CellError> {
    poirot_parsing::date::date_parts(input).map_err(|_| VALUE_ERROR)
}
/// A borrowed date-text parse plan that identifies which captured context is needed
/// before resolving a calendar date. Inspection exposes default-year and date-order
/// reads so the host can record dependencies before supplying that context.
#[derive(Clone, Copy)]
pub struct DateTextPlan<'a> {
    input: &'a str,
    /// Whether execution must supply the current year from captured context.
    pub needs_default_year: bool,
    /// Whether numeric slash-date ordering affects interpretation.
    pub reads_date_order: bool,
}
impl<'a> DateTextPlan<'a> {
    /// Inspect date grammar before the host reads optional context inputs.
    pub fn inspect(input: &'a str) -> Result<Self, CellError> {
        let parts = date_parts(input)?;
        if !matches!(parts.len(), 2 | 3) {
            return Err(VALUE_ERROR);
        }
        Ok(Self {
            input,
            needs_default_year: parts.len() == 2,
            reads_date_order: input.split(' ').next().unwrap_or("").contains('/')
                && parts.first().ok_or(VALUE_ERROR)?.len() != 4,
        })
    }
    /// Resolve calendar arguments in dependency order. Impossible fields reject
    /// under a leap-year probe before the host acquires a clock; explicit years
    /// never request the continuation. February 29 remains year-dependent.
    pub fn coerce_resolve<E: From<CellError>>(
        self,
        order: DateOrder,
        default_year: impl FnOnce() -> Result<i64, E>,
    ) -> Result<Date, E> {
        self.resolve(2000, order)?;
        let year = if self.needs_default_year {
            default_year()?
        } else {
            2000
        };
        self.resolve(year, order).map_err(Into::into)
    }
    /// Resolve inspected date text under explicit year and numeric component ordering.
    pub fn resolve(self, default_year: i64, order: DateOrder) -> Result<Date, CellError> {
        parse_date(self.input, default_year, order)
    }
}
/// Parse supported date spellings using caller-supplied default year and date order.
pub fn parse_date(s: &str, year: i64, order: DateOrder) -> Result<Date, CellError> {
    let slash = s.split(' ').next().unwrap_or("").contains('/');
    let parts = date_parts(s)?;
    let (y, m, d) = match parts.as_slice() {
        [a, b] => {
            if order == DateOrder::DayMonthYear && slash {
                (year, parse_month(b), a.parse::<i64>().ok())
            } else {
                (year, parse_month(a), b.parse::<i64>().ok())
            }
        }
        [a, b, c] if a.len() == 4 => (
            a.parse().map_err(|_| VALUE_ERROR)?,
            parse_month(b),
            c.parse().ok(),
        ),
        [a, b, c] if c.len() == 4 && slash && order == DateOrder::MonthDayYear => (
            c.parse().map_err(|_| VALUE_ERROR)?,
            parse_month(a),
            b.parse().ok(),
        ),
        [a, b, c] if c.len() == 4 => (
            c.parse().map_err(|_| VALUE_ERROR)?,
            parse_month(b),
            a.parse().ok(),
        ),
        _ => return Err(VALUE_ERROR),
    };
    let day_part = if parts.len() == 2 {
        if order == DateOrder::DayMonthYear && slash {
            // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
            parts[0]
        } else {
            // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
            parts[1]
        }
    // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
    } else if parts[0].len() == 4 || (slash && order == DateOrder::MonthDayYear) {
        // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
        if parts[0].len() == 4 {
            // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
            parts[2]
        } else {
            // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
            parts[1]
        }
    } else {
        // SAFETY: The preceding exhaustive slice match returns for every length except two or three; the two-element branch only uses indices zero/one.
        parts[0]
    };
    if day_part.len() > 2 || !day_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(VALUE_ERROR);
    }
    let d = Date {
        y,
        m: m.ok_or(VALUE_ERROR)?,
        d: d.ok_or(VALUE_ERROR)?,
    };
    if !(1..=9999).contains(&d.y)
        || !(1..=12).contains(&d.m)
        || d.d < 1
        || d.d > month_days(d.y, d.m)
    {
        Err(VALUE_ERROR)
    } else {
        Ok(d)
    }
}
