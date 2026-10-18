//! nom lexical tokens, including quoted sheet-span prefixes.
use nom::{
    IResult, Parser as NomParser,
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::{char as token_char, digit1, one_of},
    combinator::{map, map_res, opt, recognize},
    multi::fold_many0,
    sequence::delimited,
};

/// Consume optional formula whitespace without changing the remaining token.
pub(super) fn whitespace(input: &str) -> IResult<&str, &str> {
    take_while(char::is_whitespace).parse(input)
}
/// Parse an unquoted formula identifier.
pub(super) fn identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '\\')).parse(input)
}
/// Recognize a numeric literal, including an optional exponent.
pub(super) fn number_token(input: &str) -> IResult<&str, f64> {
    map_res(
        recognize((
            alt((
                // Keep the decimal point even when the optional fractional
                // digits end at EOF; digit0's EOF span can lose this suffix.
                recognize((digit1, opt((token_char('.'), opt(digit1))))),
                recognize((token_char('.'), digit1)),
            )),
            opt((one_of("eE"), opt(one_of("+-")), digit1)),
        )),
        str::parse::<f64>,
    )
    .parse(input)
}
/// Decode quoted text and its doubled-quote escapes.
pub(super) fn quoted_token(input: &str, quote: char) -> IResult<&str, String> {
    let doubled = format!("{quote}{quote}");
    delimited(
        token_char(quote),
        fold_many0(
            alt((
                map(tag(doubled.as_str()), |_| quote.to_string()),
                map(take_while1(|c| c != quote), str::to_owned),
            )),
            String::new,
            |mut output, part| {
                output.push_str(&part);
                output
            },
        ),
        token_char(quote),
    )
    .parse(input)
}

/// Parse a quoted or unquoted sheet-name token.
pub(super) fn sheet_name_token(input: &str) -> IResult<&str, String> {
    alt((
        |input| quoted_token(input, '\''),
        map(identifier, str::to_owned),
    ))
    .parse(input)
}
/// Parse the ordered sheet qualifiers preceding a 3D reference.
pub(super) fn sheet_span_prefix(input: &str) -> IResult<&str, (String, String)> {
    alt((
        map(
            (
                map(identifier, str::to_owned),
                token_char(':'),
                sheet_name_token,
                token_char('!'),
            ),
            |(first, _, last, _)| (first, last),
        ),
        map_res(
            (|input| quoted_token(input, '\''), token_char('!')),
            |(names, _)| {
                let (first, last) = names.split_once(':').ok_or(())?;
                if first.is_empty() || last.is_empty() || last.contains(':') {
                    return Err(());
                }
                Ok::<_, ()>((first.to_owned(), last.to_owned()))
            },
        ),
    ))
    .parse(input)
}
