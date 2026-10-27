/// Role of a separator or affix in a numeric input locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberLocaleToken {
    /// Separates the integer and fractional parts.
    Decimal,
    /// Separates groups of integer digits.
    Grouping,
    /// Identifies a currency amount.
    Currency,
    /// Marks a percentage amount.
    Percent,
    /// Separates elapsed-time fields.
    Time,
}
