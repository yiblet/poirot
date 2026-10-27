/// Numeric slash-date parsing policy. Formula grammar remains canonical OOXML syntax.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DateOrder {
    /// Interpret slash dates as month, day, then year.
    MonthDayYear,
    /// Interpret slash dates as day, month, then year.
    DayMonthYear,
}
