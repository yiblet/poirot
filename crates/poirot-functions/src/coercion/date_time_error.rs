/// A numeric-text datetime continuation requires an unsupported context or calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DateTimeError {
    /// The supplied locale cannot provide the pinned formatter's date names.
    #[error("datetime formatter locale is unavailable")]
    FormatterLocale,
    /// Date parsing needs a fixed timezone offset within eighteen hours of UTC.
    #[error("date parsing requires a supported fixed timezone offset")]
    DateTimezone,
    /// An omitted year would use a default outside the supported calendar.
    #[error("default datetime year {year} is outside the supported range 1..=10000")]
    DefaultYearOutOfRange {
        /// Context-provided default year rejected by calendar admission.
        year: i32,
    },
    /// An explicit extended year exceeds the calendar conversion's proven range.
    #[error("datetime year {year} exceeds the supported calendar maximum 10000")]
    ExtendedYear {
        /// Explicit input year requiring an unsupported calendar conversion.
        year: i32,
    },
}
