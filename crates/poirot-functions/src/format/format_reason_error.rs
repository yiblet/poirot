//! Specific rejected number-format conditions.

/// Typed condition that prevented number-format compilation or rendering.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatReasonError {
    /// UTF-8 formatting cannot represent unpaired UTF-16 code units.
    #[error("UTF-8 formatting cannot represent unpaired UTF-16 code units")]
    Utf8FormattingCannotRepresentUnpairedUtf16CodeUnits,
    /// Date serial outside supported nonnegative calendar range.
    #[error("date serial outside supported nonnegative calendar range")]
    DateSerialOutsideSupportedNonnegativeCalendarRange,
    /// Embedded directives among numeric placeholders unsupported.
    #[error("embedded directives among numeric placeholders unsupported")]
    EmbeddedDirectivesAmongNumericPlaceholdersUnsupported,
    /// Exponent width exceeds resource limit.
    #[error("exponent width exceeds resource limit")]
    ExponentWidthExceedsResourceLimit,
    /// Fill requires explicit character width.
    #[error("fill requires explicit character width")]
    FillRequiresExplicitCharacterWidth,
    /// Fixed fraction denominator exceeds u32 range.
    #[error("fixed fraction denominator exceeds u32 range")]
    FixedFractionDenominatorExceedsU32Range,
    /// Format exceeds resource limit.
    #[error("format exceeds resource limit")]
    FormatExceedsResourceLimit,
    /// Format has no numeric sections.
    #[error("format has no numeric sections")]
    FormatHasNoNumericSections,
    /// Format has no sections.
    #[error("format has no sections")]
    FormatHasNoSections,
    /// Formatted text count overflow.
    #[error("formatted text count overflow")]
    FormattedTextCountOverflow,
    /// Formatted text exceeds cell limit.
    #[error("formatted text exceeds cell limit")]
    FormattedTextExceedsCellLimit,
    /// Fraction denominator requires one to four placeholders.
    #[error("fraction denominator requires one to four placeholders")]
    FractionDenominatorRequiresOneToFourPlaceholders,
    /// Fraction has no numerator.
    #[error("fraction has no numerator")]
    FractionHasNoNumerator,
    /// Fraction integer exceeds supported exact range.
    #[error("fraction integer exceeds supported exact range")]
    FractionIntegerExceedsSupportedExactRange,
    /// Fraction magnitude exceeds exact integer range.
    #[error("fraction magnitude exceeds exact integer range")]
    FractionMagnitudeExceedsExactIntegerRange,
    /// Fraction numerator width exceeds limit.
    #[error("fraction numerator width exceeds limit")]
    FractionNumeratorWidthExceedsLimit,
    /// Fraction padding exceeds cell limit.
    #[error("fraction padding exceeds cell limit")]
    FractionPaddingExceedsCellLimit,
    /// Fraction percent/comma scaling is unsupported.
    #[error("fraction percent/comma scaling is unsupported")]
    FractionPercentCommaScalingIsUnsupported,
    /// Fraction requires one slash.
    #[error("fraction requires one slash")]
    FractionRequiresOneSlash,
    /// Fraction whole exceeds exact range.
    #[error("fraction whole exceeds exact range")]
    FractionWholeExceedsExactRange,
    /// Fraction whole overflow.
    #[error("fraction whole overflow")]
    FractionWholeOverflow,
    /// Fraction whole width exceeds limit.
    #[error("fraction whole width exceeds limit")]
    FractionWholeWidthExceedsLimit,
    /// Integer masks require only digits and literals.
    #[error("integer masks require only digits and literals")]
    IntegerMasksRequireOnlyDigitsAndLiterals,
    /// Invalid format syntax.
    #[error("invalid format syntax")]
    InvalidFormatSyntax,
    /// Invalid month.
    #[error("invalid month")]
    InvalidMonth,
    /// Invalid tracked fill position.
    #[error("invalid tracked fill position")]
    InvalidTrackedFillPosition,
    /// Invalid weekday.
    #[error("invalid weekday")]
    InvalidWeekday,
    /// Literal exceeds cell limit.
    #[error("literal exceeds cell limit")]
    LiteralExceedsCellLimit,
    /// Missing compiled system pattern.
    #[error("missing compiled system pattern")]
    MissingCompiledSystemPattern,
    /// Mixed fixed/variable fraction denominator.
    #[error("mixed fixed/variable fraction denominator")]
    MixedFixedVariableFractionDenominator,
    /// More than fifteen decimal placeholders unsupported.
    #[error("more than fifteen decimal placeholders unsupported")]
    MoreThanFifteenDecimalPlaceholdersUnsupported,
    /// More than six fractional-second digits unsupported.
    #[error("more than six fractional-second digits unsupported")]
    MoreThanSixFractionalSecondDigitsUnsupported,
    /// Multiple conditions in one section.
    #[error("multiple conditions in one section")]
    MultipleConditionsInOneSection,
    /// Multiple fill directives unsupported.
    #[error("multiple fill directives unsupported")]
    MultipleFillDirectivesUnsupported,
    /// Multiple system date/time selectors.
    #[error("multiple system date/time selectors")]
    MultipleSystemDateTimeSelectors,
    /// No conditional format section matches.
    #[error("no conditional format section matches")]
    NoConditionalFormatSectionMatches,
    /// Non-finite number.
    #[error("non-finite number")]
    NonFiniteNumber,
    /// Number formats have at most four sections.
    #[error("number formats have at most four sections")]
    NumberFormatsHaveAtMostFourSections,
    /// Number rounding exceeds supported finite range.
    #[error("number rounding exceeds supported finite range")]
    NumberRoundingExceedsSupportedFiniteRange,
    /// Numeric placeholders mixed with date format.
    #[error("numeric placeholders mixed with date format")]
    NumericPlaceholdersMixedWithDateFormat,
    /// Render byte bound overflow.
    #[error("render byte bound overflow")]
    RenderByteBoundOverflow,
    /// Rounded date exceeds supported calendar range.
    #[error("rounded date exceeds supported calendar range")]
    RoundedDateExceedsSupportedCalendarRange,
    /// Scaled number overflow.
    #[error("scaled number overflow")]
    ScaledNumberOverflow,
    /// Scaling exceeds resource limit.
    #[error("scaling exceeds resource limit")]
    ScalingExceedsResourceLimit,
    /// Scientific exponent exceeds supported finite range.
    #[error("scientific exponent exceeds supported finite range")]
    ScientificExponentExceedsSupportedFiniteRange,
    /// Token limit exceeded.
    #[error("token limit exceeded")]
    TokenLimitExceeded,
    /// Too many sections.
    #[error("too many sections")]
    TooManySections,
    /// Unknown date token.
    #[error("unknown date token")]
    UnknownDateToken,
    /// Unsupported digit placeholder.
    #[error("unsupported digit placeholder")]
    UnsupportedDigitPlaceholder,
    /// Unsupported elapsed token.
    #[error("unsupported elapsed token")]
    UnsupportedElapsedToken,
    /// Unsupported exponent width.
    #[error("unsupported exponent width")]
    UnsupportedExponentWidth,
    /// Unsupported numeric placeholder placement.
    #[error("unsupported numeric placeholder placement")]
    UnsupportedNumericPlaceholderPlacement,
    /// Unsupported numeric token order.
    #[error("unsupported numeric token order")]
    UnsupportedNumericTokenOrder,
    /// Unsupported system date/time fallback layout.
    #[error("unsupported system date/time fallback layout")]
    UnsupportedSystemDateTimeFallbackLayout,
    /// Unsupported system date/time locale.
    #[error("unsupported system date/time locale")]
    UnsupportedSystemDateTimeLocale,
    /// Unsupported tokens in compiled numeric section.
    #[error("unsupported tokens in compiled numeric section")]
    UnsupportedTokensInCompiledNumericSection,
    /// Unsupported tokens in fraction literal region.
    #[error("unsupported tokens in fraction literal region")]
    UnsupportedTokensInFractionLiteralRegion,
    /// Unterminated quote, bracket, or escape.
    #[error("unterminated quote, bracket, or escape")]
    UnterminatedQuoteBracketOrEscape,
    /// Width exceeds resource limit.
    #[error("width exceeds resource limit")]
    WidthExceedsResourceLimit,
    /// Zero fraction denominator.
    #[error("zero fraction denominator")]
    ZeroFractionDenominator,
    /// An unsupported bracket directive was present in the input.
    #[error("unsupported bracket directive [{directive}]")]
    UnsupportedBracketDirective {
        /// Rejected directive text from the format input.
        directive: String,
    },
}
