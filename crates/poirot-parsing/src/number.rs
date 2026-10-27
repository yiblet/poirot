//! Locale-parametric numeric syntax. Calendar interpretation is a host continuation.
//!
//! ```
//! use poirot_parsing::number::{FormattedNumber, NumberLocale, parse_formatted_number};
//! let locale = NumberLocale::new(",", Some("."), Some("EUR"), "%", ":").unwrap();
//! assert_eq!(parse_formatted_number("1.234,5", &locale), FormattedNumber::Number(Ok(1234.5)));
//! ```
pub use crate::number_error::NumberError;
pub use crate::number_locale_error::NumberLocaleError;
pub use crate::number_locale_token::NumberLocaleToken;

/// Complete numeric syntax result, distinguished from calendar fallback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FormattedNumber {
    /// Numeric syntax matched, including a possible invalid numeric result.
    Number(Result<f64, NumberError>),
    /// No numeric family matched; the consumer may request a calendar grammar.
    DateTime,
}
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::tag,
    character::complete::{digit0, digit1, one_of},
    combinator::{all_consuming, opt, recognize, verify},
    multi::fold_many0,
    sequence::{delimited, pair, preceded},
};
use std::borrow::Cow;

/// Validated literal separators and affixes for locale-dependent numeric input parsing.
/// The profile is supplied explicitly and never consults the process locale.
///
/// ```
/// use poirot_parsing::number::{FormattedNumber, NumberLocale, parse_formatted_number};
/// let locale = NumberLocale::new(",", Some("."), Some("EUR"), "%", ":")?;
/// assert_eq!(parse_formatted_number("1.234,5", &locale), FormattedNumber::Number(Ok(1234.5)));
/// # Ok::<(), poirot_parsing::number::NumberLocaleError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "LocaleDataV2")]
pub struct NumberLocale {
    decimal: Cow<'static, str>,
    grouping: Option<Cow<'static, str>>,
    currency: Option<Cow<'static, str>>,
    percent: Cow<'static, str>,
    time: Cow<'static, str>,
}
/// Unvalidated numeric-locale data used only during deserialization.
/// Conversion through `NumberLocale::new` rejects ambiguous or invalid tokens before
/// they can reach the numeric grammar.
#[derive(serde::Deserialize)]
struct LocaleDataV2 {
    decimal: String,
    grouping: Option<String>,
    currency: Option<String>,
    percent: String,
    time: String,
}
impl TryFrom<LocaleDataV2> for NumberLocale {
    type Error = NumberLocaleError;
    fn try_from(data: LocaleDataV2) -> Result<Self, Self::Error> {
        Self::new(
            &data.decimal,
            data.grouping.as_deref(),
            data.currency.as_deref(),
            &data.percent,
            &data.time,
        )
    }
}
impl NumberLocale {
    /// Fixed US-English input profile; independent of display formatting.
    pub const EN_US: Self = Self {
        decimal: Cow::Borrowed("."),
        grouping: Some(Cow::Borrowed(",")),
        currency: Some(Cow::Borrowed("$")),
        percent: Cow::Borrowed("%"),
        time: Cow::Borrowed(":"),
    };
    /// Decimal token used by numeric argument grammars.
    pub fn decimal_separator(&self) -> &str {
        &self.decimal
    }
    /// Optional grouping token used by numeric argument grammars.
    pub fn grouping_separator(&self) -> Option<&str> {
        self.grouping.as_deref()
    }
    /// Validate literal tokens once. Signs, exponent markers and fraction syntax
    /// are grammar. Currency literals and grouping spaces have separate admission.
    pub fn new(
        decimal: &str,
        grouping: Option<&str>,
        currency: Option<&str>,
        percent: &str,
        time: &str,
    ) -> Result<Self, NumberLocaleError> {
        let tokens = [
            (NumberLocaleToken::Decimal, Some(decimal)),
            (NumberLocaleToken::Grouping, grouping),
            (NumberLocaleToken::Currency, currency),
            (NumberLocaleToken::Percent, Some(percent)),
            (NumberLocaleToken::Time, Some(time)),
        ];
        for (i, (kind, token)) in tokens.iter().enumerate() {
            let Some(token) = token else { continue };
            if token.is_empty() {
                return Err(NumberLocaleError::EmptyToken { token: *kind });
            }
            if let Some(character) = token.chars().find(|c| c.is_control()) {
                return Err(NumberLocaleError::ControlCharacter {
                    token: *kind,
                    character,
                });
            }
            // Currency tokens are literals (for example EUR), not fragments of
            // the decimal grammar. A grouping token may itself be a space.
            if *kind != NumberLocaleToken::Currency
                && let Some(character) = token.chars().find(|c| {
                    c.is_ascii_digit() || matches!(c, '+' | '-' | '(' | ')' | '/' | 'e' | 'E')
                })
            {
                return Err(NumberLocaleError::NumericSyntax {
                    token: *kind,
                    character,
                });
            }
            if *kind != NumberLocaleToken::Grouping
                && let Some(character) = token.chars().find(|c| c.is_whitespace())
            {
                return Err(NumberLocaleError::Whitespace {
                    token: *kind,
                    character,
                });
            }
            for (other_kind, other) in tokens.iter().skip(i + 1) {
                if let Some(other) = other
                    && (token.starts_with(other) || other.starts_with(token))
                {
                    return Err(NumberLocaleError::OverlappingTokens {
                        first: *kind,
                        second: *other_kind,
                    });
                }
            }
        }
        Ok(Self {
            decimal: decimal.to_owned().into(),
            grouping: grouping.map(|s| s.to_owned().into()),
            currency: currency.map(|s| s.to_owned().into()),
            percent: percent.to_owned().into(),
            time: time.to_owned().into(),
        })
    }
}
impl Default for NumberLocale {
    fn default() -> Self {
        Self::EN_US
    }
}

/// Borrowed decimal components before locale separators are normalized for floating-point parsing.
/// Keeping source slices avoids building a normalized string for grammar alternatives that fail.
#[derive(Clone, Copy, Debug)]
struct DecimalV2<'a> {
    integer: &'a str,
    fraction: Option<&'a str>,
    exponent: Option<&'a str>,
}
fn parse_space(input: &str) -> IResult<&str, &str> {
    nom::bytes::complete::take_while(|c| c == ' ')(input)
}
fn parse_integer<'a>(input: &'a str, locale: &NumberLocale) -> IResult<&'a str, &'a str> {
    if let Some(group) = &locale.grouping {
        recognize(pair(
            digit1,
            fold_many0(preceded(tag(group.as_ref()), digit1), || (), |(), _| ()),
        ))
        .parse(input)
    } else {
        digit1(input)
    }
}
fn parse_decimal<'a>(input: &'a str, locale: &NumberLocale) -> IResult<&'a str, DecimalV2<'a>> {
    let (input, (integer, fraction)) = alt((
        |input| {
            let (input, integer) = parse_integer(input, locale)?;
            let (input, fraction) =
                opt(preceded(tag(locale.decimal.as_ref()), digit0)).parse(input)?;
            Ok((input, (integer, fraction)))
        },
        |input| {
            let (input, fraction) = preceded(tag(locale.decimal.as_ref()), digit1).parse(input)?;
            Ok((input, ("", Some(fraction))))
        },
    ))
    .parse(input)?;
    let (input, exponent) =
        opt(recognize((one_of("eE"), opt(one_of("+-")), digit1))).parse(input)?;
    Ok((
        input,
        DecimalV2 {
            integer,
            fraction,
            exponent,
        },
    ))
}
/// Decimal syntax with signs, parentheses, and currency placement reduced to one sign.
/// The same representation feeds numeric conversion for ordinary, currency, and percent input.
#[derive(Clone, Copy, Debug)]
struct SignedDecimalV2<'a> {
    negative: bool,
    decimal: DecimalV2<'a>,
}
fn parse_currency<'a>(input: &'a str, locale: &NumberLocale) -> IResult<&'a str, &'a str> {
    let Some(currency) = locale.currency.as_deref() else {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Tag,
        )));
    };
    tag(currency).parse(input)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum DecimalFamilyV2 {
    Number,
    Currency,
    Percent,
}
fn parse_signed_decimal<'a>(
    input: &'a str,
    locale: &NumberLocale,
    family: DecimalFamilyV2,
) -> IResult<&'a str, SignedDecimalV2<'a>> {
    let currency = family == DecimalFamilyV2::Currency;
    let trailing = family != DecimalFamilyV2::Percent;
    // Currency may appear on either side of the sign or after the amount.
    // Track each affix explicitly instead of stripping arbitrary punctuation.
    let (input, open) = opt(tag("(")).parse(input)?;
    let (input, _) = parse_space(input)?;
    let (input, first_sign) = opt(one_of("+-")).parse(input)?;
    let (input, _) = if currency {
        parse_space(input)?
    } else {
        (input, "")
    };
    let (input, prefix) = if currency {
        opt(|input| parse_currency(input, locale)).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = parse_space(input)?;
    let (input, second_sign) = if prefix.is_some() && first_sign.is_none() {
        opt(one_of("+-")).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = if currency {
        parse_space(input)?
    } else {
        (input, "")
    };
    let (input, decimal) = parse_decimal(input, locale)?;
    let (input, _) = parse_space(input)?;
    let (input, suffix) = if currency && prefix.is_none() {
        opt(|input| parse_currency(input, locale)).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = parse_space(input)?;
    let (input, trailing_sign) = if trailing && first_sign.is_none() && second_sign.is_none() {
        opt(one_of("+-")).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = parse_space(input)?;
    let (input, _) = if open.is_some() {
        tag(")").parse(input)?
    } else {
        (input, "")
    };
    if (currency && prefix.is_none() && suffix.is_none())
        || (open.is_some()
            && (first_sign.is_some() || second_sign.is_some() || trailing_sign.is_some()))
    {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    Ok((
        input,
        SignedDecimalV2 {
            negative: open.is_some() || first_sign.or(second_sign).or(trailing_sign) == Some('-'),
            decimal,
        },
    ))
}
fn parse_decimal_value(
    syntax: SignedDecimalV2<'_>,
    locale: &NumberLocale,
) -> Result<f64, NumberError> {
    let capacity = syntax
        .decimal
        .integer
        .len()
        .checked_add(syntax.decimal.fraction.map_or(0, str::len))
        .and_then(|n| n.checked_add(usize::from(syntax.decimal.fraction.is_some())))
        .and_then(|n| n.checked_add(syntax.decimal.exponent.map_or(0, str::len)))
        .and_then(|n| n.checked_add(usize::from(syntax.negative)))
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or(NumberError)?;
    // SAFETY: Checked additions and the explicit isize::MAX bound above rule
    // out String::with_capacity's capacity-overflow panic in this function.
    let mut normalized = String::with_capacity(capacity);
    if syntax.negative {
        normalized.push('-');
    }
    if let Some(group) = &locale.grouping {
        for part in syntax.decimal.integer.split(group.as_ref()) {
            normalized.push_str(part);
        }
    } else {
        normalized.push_str(syntax.decimal.integer);
    }
    if let Some(fraction) = syntax.decimal.fraction {
        normalized.push('.');
        normalized.push_str(fraction);
    }
    if let Some(exponent) = syntax.decimal.exponent {
        normalized.push_str(exponent);
    }
    normalized
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or(NumberError)
}
fn parse_mixed_fraction(input: &str) -> IResult<&str, Result<f64, NumberError>> {
    let (input, (sign, _, whole, _, numerator, _, denominator)) = (
        opt(one_of("+-")),
        parse_space,
        digit1,
        tag(" "),
        verify(digit1, |s: &str| s.len() <= 5),
        tag("/"),
        verify(digit1, |s: &str| s.len() <= 5),
    )
        .parse(input)?;
    let parts = (numerator.parse::<u16>(), denominator.parse::<u16>());
    let value = match parts {
        (Ok(n), Ok(d)) if n <= 32767 && d > 0 && d <= 32767 => whole
            .parse::<f64>()
            .ok()
            .map(|whole| {
                (whole + f64::from(n) / f64::from(d)) * if sign == Some('-') { -1. } else { 1. }
            })
            .filter(|n| n.is_finite())
            .ok_or(NumberError),
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
    };
    Ok((input, value))
}
fn parse_time_field(input: &str) -> IResult<&str, u32> {
    let (rest, digits) = verify(digit1, |s: &str| {
        s.len() <= 4 && !(s.starts_with('0') && s.len() > 2)
    })
    .parse(input)?;
    let value = digits.parse::<u32>().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;
    Ok((rest, value))
}
/// Parsed elapsed-time amount awaiting fractional-second conversion to a day serial.
/// The fractional spelling stays borrowed so its precision is retained until numeric conversion.
struct ElapsedV2<'a> {
    seconds: f64,
    fraction: Option<&'a str>,
}
fn parse_elapsed<'a>(input: &'a str, locale: &NumberLocale) -> IResult<&'a str, ElapsedV2<'a>> {
    let spaced_field = |input| delimited(parse_space, parse_time_field, parse_space).parse(input);
    let (input, first) = spaced_field(input)?;
    let (input, _) = tag(locale.time.as_ref()).parse(input)?;
    let (input, _) = parse_space(input)?;
    if input.is_empty() {
        return Ok((
            input,
            ElapsedV2 {
                seconds: f64::from(first) * 3600.,
                fraction: None,
            },
        ));
    }
    let (input, second) = spaced_field(input)?;
    let (input, third) =
        opt(preceded(tag(locale.time.as_ref()), opt(spaced_field))).parse(input)?;
    let (input, fraction) = opt(preceded(
        pair(tag(locale.decimal.as_ref()), parse_space),
        digit0,
    ))
    .parse(input)?;
    let (input, _) = parse_space(input)?;
    let (seconds, valid) = match (third, fraction) {
        (None, None) => (
            f64::from(first) * 3600. + f64::from(second) * 60.,
            first < 24 || second < 60,
        ),
        (None, Some(_)) => (
            f64::from(first) * 60. + f64::from(second),
            first < 60 || second < 60,
        ),
        (Some(third), _) => {
            let third = third.unwrap_or(0);
            (
                f64::from(first) * 3600. + f64::from(second) * 60. + f64::from(third),
                u8::from(first >= 24) + u8::from(second >= 60) + u8::from(third >= 60) <= 1,
            )
        }
    };
    if !valid {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Verify,
        )));
    }
    Ok((input, ElapsedV2 { seconds, fraction }))
}
fn parse_fraction_seconds(digits: &str) -> Result<f64, NumberError> {
    // Preserve the entered fraction instead of imposing millisecond rounding.
    // This matches independently recalculated Calc observations. The complete
    // elapsed grammar admitted ASCII digits; an empty fraction denotes zero.
    let capacity = digits
        .len()
        .checked_add(2)
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or(NumberError)?;
    // SAFETY: The checked addition and explicit isize::MAX bound above prove
    // the locally constructed normalization capacity cannot overflow.
    let mut normalized = String::with_capacity(capacity);
    normalized.push_str("0.");
    normalized.push_str(digits);
    normalized.parse::<f64>().map_err(|_| NumberError)
}
fn parse_elapsed_candidate<'a>(input: &'a str, locale: &NumberLocale) -> IResult<&'a str, ()> {
    // Complete numeric time candidates belong to the elapsed family. Calendar
    // parsing may still handle AM/PM and dates, but must not undo this family's
    // field bounds or leading-zero rejection after a failed attempt.
    let (rest, _) = (digit1, parse_space, tag(locale.time.as_ref())).parse(input)?;
    let (rest, _) = fold_many0(
        alt((
            digit1,
            tag(" "),
            tag(locale.time.as_ref()),
            tag(locale.decimal.as_ref()),
        )),
        || (),
        |(), _| (),
    )
    .parse(rest)?;
    Ok((rest, ()))
}
/// Parse complete numeric text under explicit locale tokens.
/// A no-match leaves calendar fallback to the consumer. The caller must admit
/// normalization storage and bounded scanner work before calling this parser.
pub fn parse_formatted_number(text: &str, locale: &NumberLocale) -> FormattedNumber {
    // Deterministic reservation for the bounded alternatives, normalization
    // and final floating-point parse. The alternatives never recurse or collect.
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    let numeric = alt((
        |input| {
            all_consuming(|input| parse_signed_decimal(input, locale, DecimalFamilyV2::Number))
                .parse(input)
        },
        |input| {
            all_consuming(|input| parse_signed_decimal(input, locale, DecimalFamilyV2::Currency))
                .parse(input)
        },
    ))
    .parse(text);
    if let Ok((_, syntax)) = numeric {
        return FormattedNumber::Number(parse_decimal_value(syntax, locale));
    }
    let percent = nom::sequence::terminated(
        pair(
            |input| parse_signed_decimal(input, locale, DecimalFamilyV2::Percent),
            parse_space,
        )
        .map(|(n, _)| n),
        tag(locale.percent.as_ref()),
    );
    if let Ok((_, syntax)) = all_consuming(percent).parse(text) {
        return FormattedNumber::Number(parse_decimal_value(syntax, locale).map(|n| n / 100.));
    }
    if text.starts_with(locale.percent.as_ref()) || text.ends_with(locale.percent.as_ref()) {
        return FormattedNumber::Number(Err(NumberError));
    }
    if let Ok((_, value)) = all_consuming(parse_mixed_fraction).parse(text) {
        return FormattedNumber::Number(value);
    }
    if let Ok((_, syntax)) = all_consuming(|input| parse_elapsed(input, locale)).parse(text) {
        let fraction = syntax.fraction.map(parse_fraction_seconds).transpose();
        return FormattedNumber::Number(
            fraction.map(|fraction| (syntax.seconds + fraction.unwrap_or(0.)) / 86400.),
        );
    }
    if all_consuming(|input| parse_elapsed_candidate(input, locale))
        .parse(text)
        .is_ok()
    {
        return FormattedNumber::Number(Err(NumberError));
    }
    FormattedNumber::DateTime
}

/// Borrow one locale-independent decimal spelling, leaving unconsumed input.
/// No dates, grouping, percent, fractions, NaN, or infinity spellings.
pub fn decimal(input: &str) -> IResult<&str, &str> {
    let (rest, _) = (
        opt(one_of("+-")),
        alt((
            (digit1, opt((tag("."), digit0))).map(|_| ()),
            (tag("."), digit1).map(|_| ()),
        )),
        opt((one_of("eE"), opt(one_of("+-")), digit1)),
    )
        .parse(input)?;
    // Derive the borrowed spelling from lengths: nom's complete digit scanner
    // can return an empty remainder whose pointer is not at the input's end.
    let spelling = input
        .len()
        .checked_sub(rest.len())
        .and_then(|end| input.get(..end))
        .ok_or_else(|| {
            nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Verify))
        })?;
    Ok((rest, spelling))
}

/// Parse a complete finite decimal with the argument grammar's ASCII whitespace.
pub fn parse_finite_decimal(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
    let (_, spelling) = all_consuming(decimal).parse(text).ok()?;
    spelling.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// The classic criteria numeric grammar, including its distinct whitespace rule.
pub fn parse_criteria_number(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    let (_, number) = all_consuming(nom::number::complete::double::<_, nom::error::Error<_>>)
        .parse(text)
        .ok()?;
    number.is_finite().then_some(number)
}

/// Complete financial decimal grammar, with source ASCII-control trimming.
pub fn parse_financial_decimal(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    let bytes = text.as_bytes();
    let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let start = at;
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    let mut digits = at - start;
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        digits += at - start;
    }
    if digits == 0 {
        return None;
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let start = at;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == start {
            return None;
        }
    }
    if at != bytes.len() {
        return None;
    }
    crate::java_number::parse_java_number(text).filter(|number| number.is_finite())
}
/// Complete statistical decimal grammar, with source ASCII-control trimming.
pub fn parse_statistics_decimal(text: &str) -> Option<f64> {
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    if text
        .bytes()
        .any(|c| !matches!(c, b'0'..=b'9' | b'.' | b'+' | b'-' | b'e' | b'E'))
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Untrimmed Rust floating grammar. Consumers choose whitespace and finite admission.
pub fn parse_rust_float(text: &str) -> Option<f64> {
    text.parse().ok()
}

/// Source decimal/scientific prefix grammar, distinct from the narrow decimal primitive.
pub fn source_float(input: &str) -> IResult<&str, &str> {
    nom::number::complete::recognize_float(input)
}

#[cfg(test)]
mod tests {
    use crate::number::NumberLocale;
    use crate::number_locale_error::NumberLocaleError;
    use crate::number_locale_token::NumberLocaleToken;

    #[test]
    fn locale_validation_reports_rejected_token_and_condition() {
        assert_eq!(
            NumberLocale::new("", None, None, "%", ":"),
            Err(NumberLocaleError::EmptyToken {
                token: NumberLocaleToken::Decimal
            })
        );
        assert_eq!(
            NumberLocale::new("\t", None, None, "%", ":"),
            Err(NumberLocaleError::ControlCharacter {
                token: NumberLocaleToken::Decimal,
                character: '\t'
            })
        );
        assert_eq!(
            NumberLocale::new("e", None, None, "%", ":"),
            Err(NumberLocaleError::NumericSyntax {
                token: NumberLocaleToken::Decimal,
                character: 'e'
            })
        );
        assert_eq!(
            NumberLocale::new(" ", None, None, "%", ":"),
            Err(NumberLocaleError::Whitespace {
                token: NumberLocaleToken::Decimal,
                character: ' '
            })
        );
        assert_eq!(
            NumberLocale::new("::", None, None, "%", ":"),
            Err(NumberLocaleError::OverlappingTokens {
                first: NumberLocaleToken::Decimal,
                second: NumberLocaleToken::Time
            })
        );
        assert!(NumberLocale::new(",", Some(" "), Some("EUR"), "%", ":").is_ok());
    }

    #[test]
    fn public_errors_support_thread_boundaries() {
        fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
        assert_error::<NumberLocaleError>();
        assert_error::<crate::number_error::NumberError>();
        assert_error::<crate::date_syntax_error::DateSyntaxError>();
    }
}
