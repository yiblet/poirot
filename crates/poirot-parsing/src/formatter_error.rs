/// Failure to match formatter input or interpret the pinned formatter grammar.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum FormatterError {
    /// A requested field slot is outside the accumulator.
    #[error("formatter field index {index} is invalid")]
    InvalidField {
        /// Requested field slot.
        index: usize,
    },
    /// A repeated input field disagrees with a previously committed value.
    #[error("formatter field {index} conflicts with its committed value")]
    ConflictingField {
        /// Conflicting field slot.
        index: usize,
    },
    /// The input does not contain the required ASCII digit run.
    #[error("formatter input requires at least {minimum} digits")]
    MissingDigits {
        /// Required digit count.
        minimum: usize,
    },
    /// The year sign does not agree with the extended-year width rule.
    #[error("formatter year sign does not match its digit width")]
    InvalidYearSign,
    /// The input spells a negative zero, which the source grammar rejects.
    #[error("formatter input contains negative zero")]
    NegativeZero,
    /// A scanned digit run cannot be represented as an intermediate integer.
    #[error("formatter integer conversion failed")]
    Integer(#[source] std::num::ParseIntError),
    /// A pinned month name cannot provide its required abbreviation.
    #[error("formatter month name lacks a three-byte abbreviation")]
    InvalidMonthName,
    /// The input does not begin with an admitted month name.
    #[error("formatter input does not match a month name")]
    MonthMismatch,
    /// The input does not begin with AM or PM.
    #[error("formatter input does not match a meridiem")]
    MeridiemMismatch,
    /// The formatter pattern requests an unsupported alphabetic token.
    #[error("formatter token byte {token} is unsupported")]
    UnsupportedToken {
        /// Unrecognized token byte.
        token: u8,
    },
    /// The pattern ends before an optional section closes.
    #[error("formatter optional section is unclosed at byte {index}")]
    UnclosedSection {
        /// Cursor at which the closing bracket was missing.
        index: usize,
    },
    /// The requested pattern section lies outside the pattern.
    #[error("formatter section {start}..{end} exceeds pattern length {length}")]
    InvalidSection {
        /// Requested first byte.
        start: usize,
        /// Requested exclusive end byte.
        end: usize,
        /// Available pattern bytes.
        length: usize,
    },
    /// The input does not begin with the pattern's literal byte.
    #[error("formatter input does not match literal byte {expected}")]
    LiteralMismatch {
        /// Literal required by the pattern.
        expected: u8,
    },
}
