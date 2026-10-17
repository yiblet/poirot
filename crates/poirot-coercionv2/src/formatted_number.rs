//! Locale-parametric numeric syntax. Calendar interpretation is a host continuation.
use crate::CellError;
use crate::compatibility::NumericTextDecision;
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

/// Explicit numeric tokens; these never consult the process locale.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "LocaleDataV2")]
pub struct NumberLocaleV2 {
    decimal: Cow<'static, str>,
    grouping: Option<Cow<'static, str>>,
    currency: Option<Cow<'static, str>>,
    percent: Cow<'static, str>,
    time: Cow<'static, str>,
}
#[derive(serde::Deserialize)]
struct LocaleDataV2 {
    decimal: String,
    grouping: Option<String>,
    currency: Option<String>,
    percent: String,
    time: String,
}
impl TryFrom<LocaleDataV2> for NumberLocaleV2 {
    type Error = &'static str;
    fn try_from(data: LocaleDataV2) -> Result<Self, Self::Error> {
        Self::new(
            &data.decimal,
            data.grouping.as_deref(),
            data.currency.as_deref(),
            &data.percent,
            &data.time,
        )
        .ok_or("numeric locale tokens are empty, overlap, or contain numeric syntax")
    }
}
impl NumberLocaleV2 {
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
    ) -> Option<Self> {
        let tokens = [Some(decimal), grouping, currency, Some(percent), Some(time)];
        for (i, token) in tokens.iter().enumerate() {
            let Some(token) = token else { continue };
            // Currency tokens are literals (for example EUR), not fragments of
            // the decimal grammar. A grouping token may itself be a space.
            if token.is_empty()
                || token.chars().any(|c| c.is_control())
                || (i != 2
                    && token.chars().any(|c| {
                        c.is_ascii_digit() || matches!(c, '+' | '-' | '(' | ')' | '/' | 'e' | 'E')
                    }))
                || (i != 1 && token.chars().any(char::is_whitespace))
            {
                return None;
            }
            if tokens
                .iter()
                .skip(i + 1)
                .flatten()
                .any(|other| token.starts_with(other) || other.starts_with(token))
            {
                return None;
            }
        }
        Some(Self {
            decimal: decimal.to_owned().into(),
            grouping: grouping.map(|s| s.to_owned().into()),
            currency: currency.map(|s| s.to_owned().into()),
            percent: percent.to_owned().into(),
            time: time.to_owned().into(),
        })
    }
}
impl Default for NumberLocaleV2 {
    fn default() -> Self {
        Self::EN_US
    }
}

#[derive(Clone, Copy, Debug)]
struct DecimalV2<'a> {
    integer: &'a str,
    fraction: Option<&'a str>,
    exponent: Option<&'a str>,
}
fn coercev2_space(input: &str) -> IResult<&str, &str> {
    nom::bytes::complete::take_while(|c| c == ' ')(input)
}
fn coercev2_integer<'a>(input: &'a str, locale: &NumberLocaleV2) -> IResult<&'a str, &'a str> {
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
fn coercev2_decimal<'a>(
    input: &'a str,
    locale: &NumberLocaleV2,
) -> IResult<&'a str, DecimalV2<'a>> {
    let (input, (integer, fraction)) = alt((
        |input| {
            let (input, integer) = coercev2_integer(input, locale)?;
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
#[derive(Clone, Copy, Debug)]
struct SignedDecimalV2<'a> {
    negative: bool,
    decimal: DecimalV2<'a>,
}
fn coercev2_currency<'a>(input: &'a str, locale: &NumberLocaleV2) -> IResult<&'a str, &'a str> {
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
fn coercev2_signed_decimal<'a>(
    input: &'a str,
    locale: &NumberLocaleV2,
    family: DecimalFamilyV2,
) -> IResult<&'a str, SignedDecimalV2<'a>> {
    let currency = family == DecimalFamilyV2::Currency;
    let trailing = family != DecimalFamilyV2::Percent;
    // Currency may appear on either side of the sign or after the amount.
    // Track each affix explicitly instead of stripping arbitrary punctuation.
    let (input, open) = opt(tag("(")).parse(input)?;
    let (input, _) = coercev2_space(input)?;
    let (input, first_sign) = opt(one_of("+-")).parse(input)?;
    let (input, _) = if currency {
        coercev2_space(input)?
    } else {
        (input, "")
    };
    let (input, prefix) = if currency {
        opt(|input| coercev2_currency(input, locale)).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = coercev2_space(input)?;
    let (input, second_sign) = if prefix.is_some() && first_sign.is_none() {
        opt(one_of("+-")).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = if currency {
        coercev2_space(input)?
    } else {
        (input, "")
    };
    let (input, decimal) = coercev2_decimal(input, locale)?;
    let (input, _) = coercev2_space(input)?;
    let (input, suffix) = if currency && prefix.is_none() {
        opt(|input| coercev2_currency(input, locale)).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = coercev2_space(input)?;
    let (input, trailing_sign) = if trailing && first_sign.is_none() && second_sign.is_none() {
        opt(one_of("+-")).parse(input)?
    } else {
        (input, None)
    };
    let (input, _) = coercev2_space(input)?;
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
fn coercev2_decimal_value(
    syntax: SignedDecimalV2<'_>,
    locale: &NumberLocaleV2,
) -> Result<f64, CellError> {
    let capacity = syntax
        .decimal
        .integer
        .len()
        .checked_add(syntax.decimal.fraction.map_or(0, str::len))
        .and_then(|n| n.checked_add(usize::from(syntax.decimal.fraction.is_some())))
        .and_then(|n| n.checked_add(syntax.decimal.exponent.map_or(0, str::len)))
        .and_then(|n| n.checked_add(usize::from(syntax.negative)))
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or(CellError::Value)?;
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
        .ok_or(CellError::Value)
}
fn coercev2_mixed_fraction(input: &str) -> IResult<&str, Result<f64, CellError>> {
    let (input, (sign, _, whole, _, numerator, _, denominator)) = (
        opt(one_of("+-")),
        coercev2_space,
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
            .ok_or(CellError::Value),
        _ => {
            return Err(nom::Err::Error(nom::error::Error::new(
                input,
                nom::error::ErrorKind::Verify,
            )));
        }
    };
    Ok((input, value))
}
fn coercev2_time_field(input: &str) -> IResult<&str, u32> {
    let (rest, digits) = verify(digit1, |s: &str| {
        s.len() <= 4 && !(s.starts_with('0') && s.len() > 2)
    })
    .parse(input)?;
    let value = digits.parse::<u32>().map_err(|_| {
        nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::Digit))
    })?;
    Ok((rest, value))
}
struct ElapsedV2<'a> {
    seconds: f64,
    fraction: Option<&'a str>,
}
fn coercev2_elapsed<'a>(
    input: &'a str,
    locale: &NumberLocaleV2,
) -> IResult<&'a str, ElapsedV2<'a>> {
    let spaced_field =
        |input| delimited(coercev2_space, coercev2_time_field, coercev2_space).parse(input);
    let (input, first) = spaced_field(input)?;
    let (input, _) = tag(locale.time.as_ref()).parse(input)?;
    let (input, _) = coercev2_space(input)?;
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
        pair(tag(locale.decimal.as_ref()), coercev2_space),
        digit0,
    ))
    .parse(input)?;
    let (input, _) = coercev2_space(input)?;
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
fn coercev2_fraction_seconds(digits: &str) -> Result<f64, CellError> {
    // Preserve the entered fraction instead of imposing millisecond rounding.
    // This matches independently recalculated Calc observations. The complete
    // elapsed grammar admitted ASCII digits; an empty fraction denotes zero.
    let capacity = digits
        .len()
        .checked_add(2)
        .filter(|n| *n <= isize::MAX as usize)
        .ok_or(CellError::Value)?;
    // SAFETY: The checked addition and explicit isize::MAX bound above prove
    // the locally constructed normalization capacity cannot overflow.
    let mut normalized = String::with_capacity(capacity);
    normalized.push_str("0.");
    normalized.push_str(digits);
    normalized.parse::<f64>().map_err(|_| CellError::Value)
}
fn coercev2_elapsed_candidate<'a>(input: &'a str, locale: &NumberLocaleV2) -> IResult<&'a str, ()> {
    // Complete numeric time candidates belong to the elapsed family. Calendar
    // parsing may still handle AM/PM and dates, but must not undo this family's
    // field bounds or leading-zero rejection after a failed attempt.
    let (rest, _) = (digit1, coercev2_space, tag(locale.time.as_ref())).parse(input)?;
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
/// Parse one admitted text argument. The callback admits normalization storage
/// and bounded scanner work before allocation or scanning; failed alternatives
/// retain that work. A no-match leaves calendar fallback to the consumer.
pub fn coercev2_formatted_number<E>(
    text: &str,
    locale: &NumberLocaleV2,
    mut admit: impl FnMut(usize, u64) -> Result<(), E>,
) -> Result<NumericTextDecision, E> {
    // Deterministic reservation for the bounded alternatives, normalization
    // and final floating-point parse. The alternatives never recurse or collect.
    admit(text.len(), (text.len() as u64).saturating_mul(12))?;
    let text = text.trim_matches(|c: char| c <= '\u{20}');
    let numeric = alt((
        |input| {
            all_consuming(|input| coercev2_signed_decimal(input, locale, DecimalFamilyV2::Number))
                .parse(input)
        },
        |input| {
            all_consuming(|input| coercev2_signed_decimal(input, locale, DecimalFamilyV2::Currency))
                .parse(input)
        },
    ))
    .parse(text);
    if let Ok((_, syntax)) = numeric {
        return Ok(NumericTextDecision::Number(coercev2_decimal_value(
            syntax, locale,
        )));
    }
    let percent = nom::sequence::terminated(
        pair(
            |input| coercev2_signed_decimal(input, locale, DecimalFamilyV2::Percent),
            coercev2_space,
        )
        .map(|(n, _)| n),
        tag(locale.percent.as_ref()),
    );
    if let Ok((_, syntax)) = all_consuming(percent).parse(text) {
        return Ok(NumericTextDecision::Number(
            coercev2_decimal_value(syntax, locale).map(|n| n / 100.),
        ));
    }
    if text.starts_with(locale.percent.as_ref()) || text.ends_with(locale.percent.as_ref()) {
        return Ok(NumericTextDecision::Number(Err(CellError::Value)));
    }
    if let Ok((_, value)) = all_consuming(coercev2_mixed_fraction).parse(text) {
        return Ok(NumericTextDecision::Number(value));
    }
    if let Ok((_, syntax)) = all_consuming(|input| coercev2_elapsed(input, locale)).parse(text) {
        let fraction = syntax.fraction.map(coercev2_fraction_seconds).transpose();
        return Ok(NumericTextDecision::Number(fraction.map(|fraction| {
            (syntax.seconds + fraction.unwrap_or(0.)) / 86400.
        })));
    }
    if all_consuming(|input| coercev2_elapsed_candidate(input, locale))
        .parse(text)
        .is_ok()
    {
        return Ok(NumericTextDecision::Number(Err(CellError::Value)));
    }
    Ok(NumericTextDecision::DateTime)
}
