//! Render cell values with Excel number formats.
//!
//! Compile a format once with [`compile_format`](crate::format::compile_format), then use [`render_value`](crate::format::render_value) for each
//! value. Formats can contain numeric, date, time, and text sections. Rendering
//! changes the displayed text, not the stored cell value.
//!
//! ```rust
//! fn main() -> Result<(), poirot_functions::format::FormatCompileError> {
//!     use poirot_functions::format::{compile_format, render_value, FormatCompilePolicy, FormatRenderContext};
//!     use poirot_functions::scalar::CellValue;
//!
//!     let format = compile_format("0.00", FormatCompilePolicy::default())?;
//!     let displayed = render_value(
//!         &CellValue::Number(12.5), &format, FormatRenderContext::default(),
//!     )?;
//!     assert_eq!(displayed.text, "12.50");
//!     Ok(())
//! }
//! ```
//!
//! Unsupported formats return errors. Date rendering uses the supplied workbook
//! calendar; locale and width are explicit rendering options.
use crate::scalar::{CellValue, DateSystem};
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag_no_case, take_until, take_while1},
    character::complete::{anychar, char},
    combinator::{map, value, verify},
    multi::{many0, separated_list1},
    sequence::{delimited, preceded},
};

mod system;
/// Pinned system date/time selectors resolved independently of host locale.
pub use system::SystemDateTime;
mod format_compile_error;
pub use format_compile_error::FormatCompileError;
mod format_error;
mod format_error_kind;
mod format_reason_error;
// Structured syntax, fidelity, and resource diagnostics from format compilation/rendering.
pub use format_error::FormatError;
use format_error::{error, fidelity_error, resource_error};
pub use format_error_kind::FormatErrorKind;
pub use format_reason_error::FormatReasonError;
/// One element of Excel number-format syntax, such as a digit placeholder or date field.
/// These tokens describe display formatting, not the formula language.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Text copied directly into formatted output.
    Literal(String),
    /// Windows regional system date/time selector, resolved only at rendering.
    SystemDateTime(SystemDateTime),
    /// Numeric placeholder controlling digit or padding emission.
    Digit(char),
    /// Decimal separator in a numeric layout.
    Decimal,
    /// Grouping or scaling comma, interpreted during compilation.
    Comma,
    /// Percent marker that also scales the numeric value.
    Percent,
    /// Text substitution marker.
    Text,
    /// General numeric formatting selector.
    General,
    /// Date/time field whose role is resolved by its surrounding layout.
    Date(char, usize),
    /// Twelve-hour clock meridiem marker.
    AmPm(bool),
    /// Elapsed-time field that may exceed calendar field ranges.
    Elapsed(char, usize),
    /// Width-preserving padding directive.
    Padding(char),
    /// Repeat a character to fill the requested output width.
    Fill(char),
    /// Fraction separator retained for fraction-plan compilation.
    Slash,
    /// Scientific exponent layout.
    Exponent {
        /// Whether nonnegative exponents include an explicit plus sign.
        positive_sign: bool,
        /// Minimum exponent digit count.
        digits: usize,
    },
}
/// Numeric predicate selecting a conditional number-format section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Condition {
    /// Select values below the stored threshold.
    Less(f64),
    /// Select values at or below the stored threshold.
    LessEqual(f64),
    /// Select values equal to the stored threshold.
    Equal(f64),
    /// Select values unequal to the stored threshold.
    NotEqual(f64),
    /// Select values above the stored threshold.
    Greater(f64),
    /// Select values at or above the stored threshold.
    GreaterEqual(f64),
}
impl Condition {
    fn matches(self, n: f64) -> bool {
        match self {
            Self::Less(v) => n < v,
            Self::LessEqual(v) => n <= v,
            Self::Equal(v) => n == v,
            Self::NotEqual(v) => n != v,
            Self::Greater(v) => n > v,
            Self::GreaterEqual(v) => n >= v,
        }
    }
}
/// One semicolon-separated number-format branch with tokens, optional condition, and color.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Ordered tokens belonging to one semicolon-separated format section.
    pub tokens: Vec<Token>,
    /// Optional numeric predicate selecting this section.
    pub condition: Option<Condition>,
    /// Optional display color retained alongside rendered text.
    pub color: Option<String>,
}
/// Parsed number-format sections. Without conditions, sections conventionally select
/// positive, negative, zero, and text values; [`format_value`] chooses the applicable branch.
#[derive(Debug, Clone, PartialEq)]
pub struct Format {
    /// Parsed sections before layout compilation.
    pub sections: Vec<Section>,
}
/// Per-render workbook date system and optional character width for `*` fill.
/// Pass the source workbook's date system when displaying date serials.
#[derive(Debug, Clone, Copy, Default)]
pub struct FormatOptions {
    /// Workbook epoch used when formatting date serials.
    pub date_system: DateSystem,
    /// Width measured in Unicode characters, not pixels.
    pub width: Option<usize>,
}
/// Rendered display text and optional format color. The original cell value is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedValue {
    /// Rendered display text.
    pub text: String,
    /// Selected section's optional display color.
    pub color: Option<String>,
}
/// Intermediate format token before section directives and literals are resolved.
#[derive(Debug, Clone)]
enum Raw {
    Token(Token),
    Bracket(String),
    Date(String),
}
fn quoted(input: &str) -> IResult<&str, String> {
    map(
        delimited(
            char('"'),
            many0(alt((
                map(preceded(char('\\'), anychar), |c| c.to_string()),
                map(take_while1(|c| c != '"' && c != '\\'), str::to_owned),
            ))),
            char('"'),
        ),
        |parts| parts.concat(),
    )
    .parse(input)
}
fn token(input: &str) -> IResult<&str, Raw> {
    if input.starts_with(';') {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Char,
        )));
    }
    alt((
        map(quoted, |s| Raw::Token(Token::Literal(s))),
        map(
            delimited(char('['), take_until("]"), char(']')),
            |s: &str| Raw::Bracket(s.into()),
        ),
        map(preceded(char('\\'), anychar), |c| {
            Raw::Token(Token::Literal(c.to_string()))
        }),
        map(preceded(char('_'), anychar), |c| {
            Raw::Token(Token::Padding(c))
        }),
        map(preceded(char('*'), anychar), |c| Raw::Token(Token::Fill(c))),
        value(Raw::Token(Token::AmPm(true)), tag_no_case("AM/PM")),
        value(Raw::Token(Token::AmPm(false)), tag_no_case("A/P")),
        value(Raw::Token(Token::General), tag_no_case("General")),
        map(
            (
                nom::character::complete::one_of("Ee"),
                nom::character::complete::one_of("+-"),
                take_while1(|c| c == '0'),
            ),
            |(_, sign, d): (_, _, &str)| {
                Raw::Token(Token::Exponent {
                    positive_sign: sign == '+',
                    digits: d.len(),
                })
            },
        ),
        map(
            take_while1(|c: char| matches!(c.to_ascii_lowercase(), 'y' | 'm' | 'd' | 'h' | 's')),
            |s: &str| Raw::Date(s.into()),
        ),
        map(
            verify(anychar, |c| !matches!(c, '"' | '[' | '\\' | '_' | '*')),
            |c| {
                Raw::Token(match c {
                    '0' | '#' | '?' => Token::Digit(c),
                    '.' => Token::Decimal,
                    ',' => Token::Comma,
                    '%' => Token::Percent,
                    '@' => Token::Text,
                    '/' => Token::Slash,
                    _ => Token::Literal(c.to_string()),
                })
            },
        ),
    ))
    .parse(input)
}
/// Parse up to four sections; unsupported bracket directives are explicit errors.
pub fn parse_format(input: &str) -> Result<Format, FormatError> {
    if input.len() > 32768 {
        return Err(error(
            crate::format::FormatReasonError::FormatExceedsResourceLimit,
        ));
    }
    let (rest, raw) = separated_list1(char(';'), many0(token))
        .parse(input)
        .map_err(|_| error(crate::format::FormatReasonError::InvalidFormatSyntax))?;
    if !rest.is_empty() {
        return Err(FormatError {
            kind: FormatErrorKind::InvalidFormat,
            position: input.len() - rest.len(),
            reason: crate::format::FormatReasonError::UnterminatedQuoteBracketOrEscape,
        });
    }
    if raw.len() > 4 {
        return Err(error(
            crate::format::FormatReasonError::NumberFormatsHaveAtMostFourSections,
        ));
    }
    let mut sections = Vec::new();
    for parts in raw {
        let mut section = Section {
            tokens: Vec::new(),
            condition: None,
            color: None,
        };
        for part in parts {
            match part {
                Raw::Date(s) => {
                    let mut run = String::new();
                    for c in s.chars() {
                        if run
                            .chars()
                            .next()
                            .is_some_and(|r: char| !r.eq_ignore_ascii_case(&c))
                        {
                            // SAFETY: The run guard requires a first character or a nonempty run; the elapsed-token branch matches only nonempty ASCII spellings.
                            let first = run.chars().next().unwrap().to_ascii_lowercase();
                            section.tokens.push(Token::Date(first, run.len()));
                            run.clear()
                        }
                        run.push(c)
                    }
                    if !run.is_empty() {
                        section.tokens.push(Token::Date(
                            // SAFETY: The run guard requires a first character or a nonempty run; the elapsed-token branch matches only nonempty ASCII spellings.
                            run.chars().next().unwrap().to_ascii_lowercase(),
                            run.len(),
                        ))
                    }
                }
                Raw::Token(t) => section.tokens.push(t),
                Raw::Bracket(s) => {
                    let lower = s.to_ascii_lowercase();
                    if let Some(selector) = system::selector(&lower) {
                        section.tokens.push(Token::SystemDateTime(selector));
                        continue;
                    }
                    if matches!(lower.as_str(), "h" | "hh" | "m" | "mm" | "s" | "ss") {
                        section
                            .tokens
                            // SAFETY: The run guard requires a first character or a nonempty run; the elapsed-token branch matches only nonempty ASCII spellings.
                            .push(Token::Elapsed(lower.chars().next().unwrap(), lower.len()));
                        continue;
                    }
                    if let Some(condition) = parse_condition(&s) {
                        if section.condition.replace(condition).is_some() {
                            return Err(error(
                                crate::format::FormatReasonError::MultipleConditionsInOneSection,
                            ));
                        }
                        continue;
                    }
                    if matches!(
                        lower.as_str(),
                        "black"
                            | "white"
                            | "red"
                            | "green"
                            | "blue"
                            | "yellow"
                            | "magenta"
                            | "cyan"
                    ) || lower.strip_prefix("color").is_some_and(|n| {
                        n.trim().parse::<u8>().is_ok_and(|v| (1..=56).contains(&v))
                    }) {
                        section.color = Some(s);
                        continue;
                    }
                    return Err(error(FormatReasonError::UnsupportedBracketDirective {
                        directive: s,
                    }));
                }
            }
        }
        sections.push(section)
    }
    Ok(Format { sections })
}
fn parse_condition(input: &str) -> Option<Condition> {
    let mut parser = (
        alt((
            nom::bytes::complete::tag(">="),
            nom::bytes::complete::tag("<="),
            nom::bytes::complete::tag("<>"),
            nom::bytes::complete::tag("="),
            nom::bytes::complete::tag(">"),
            nom::bytes::complete::tag("<"),
        )),
        nom::character::complete::space0,
        nom::number::complete::double,
        nom::character::complete::space0,
    );
    let result: IResult<&str, (&str, &str, f64, &str)> = parser.parse(input);
    let (rest, (op, _, n, _)) = result.ok()?;
    if !rest.is_empty() || !n.is_finite() {
        return None;
    }
    Some(match op {
        ">=" => Condition::GreaterEqual(n),
        "<=" => Condition::LessEqual(n),
        "<>" => Condition::NotEqual(n),
        "=" => Condition::Equal(n),
        ">" => Condition::Greater(n),
        _ => Condition::Less(n),
    })
}
/// Format a typed value. Empty sections intentionally hide a value.
pub fn format_value(
    value: &CellValue,
    format: &Format,
    options: FormatOptions,
) -> Result<FormattedValue, FormatError> {
    let plan = compile_program(format, FormatCompilePolicy::default()).map_err(|e| e.source)?;
    render_value(value, &plan, options.into())
}

mod output;
use output::RenderedText;
fn render_text(tokens: &[Token], value: &str) -> RenderedText {
    let mut out = RenderedText::default();
    for token in tokens {
        match token {
            Token::Text | Token::General => out.push_str(value),
            Token::Literal(_) | Token::Fill(_) | Token::Padding(_) | Token::Percent => {
                out.push_literal(token)
            }
            _ => {}
        }
    }
    out
}
fn literal(t: &Token) -> String {
    match t {
        Token::Literal(s) => s.clone(),
        Token::Fill(_) => String::new(),
        Token::Padding(_) => " ".into(),
        Token::Percent => "%".into(),
        Token::Slash => "/".into(),
        Token::Comma => ",".into(),
        Token::Decimal => ".".into(),
        _ => String::new(),
    }
}
mod date;
// Bounded calendar conversion and explicit handling of Excel's fictional leap day.
pub use date::{CalendarDate, CalendarPolicy, PhantomDayPolicy};
mod number;
mod plan;
// Reusable number layouts compiled before value-dependent rendering.
pub use number::{NumberLayout, NumberPlan};
// Pure format compilation and bounded execution of immutable rendering programs.
pub use plan::{
    FormatCompilePolicy, FormatLocale, FormatPlan, FormatRenderContext, SectionKind, SectionPlan,
    compile_format, compile_program, render_value,
};

#[cfg(test)]
mod tests {
    use super::*;
    fn render(n: f64, pattern: &str) -> String {
        format_value(
            &CellValue::Number(n),
            &parse_format(pattern).unwrap(),
            FormatOptions::default(),
        )
        .unwrap()
        .text
    }
    #[test]
    fn number_placeholders_grouping_percent_scaling() {
        for (n, f, want) in [
            (1234.567, "#,##0.00", "1,234.57"),
            (0.5, ".00", ".50"),
            (2., "0.", "2."),
            (0.125, "0.0%", "12.5%"),
            (1250000., "0.00,,", "1.25"),
            (2., "0.##", "2"),
            (2., "0.00", "2.00"),
            (0.5, "#.##", "0.5"),
            (3., "000", "003"),
            (1.005, "0.00", "1.01"),
            (-1234., "#,##0;(#,##0)", "(1,234)"),
            (0., "0;[Red](0);\"zero\"", "zero"),
        ] {
            assert_eq!(render(n, f), want, "{n} {f}");
        }
    }
    #[test]
    fn sections_conditions_colors_text_literals() {
        let f = parse_format("[Red][>=100]0.0;[Blue][<0](0);0;\"name: \"@").unwrap();
        let v = format_value(&CellValue::Number(101.), &f, FormatOptions::default()).unwrap();
        assert_eq!(v.text, "101.0");
        assert_eq!(v.color.as_deref(), Some("Red"));
        assert_eq!(
            format_value(&CellValue::Text("Ada".into()), &f, FormatOptions::default())
                .unwrap()
                .text,
            "name: Ada"
        );
        assert_eq!(render(4., "\"m\"0\\!"), "m4!");
        assert_eq!(render(4., "0;"), "4");
        assert_eq!(render(-4., "0;"), "");
        assert_eq!(render(1., "\"a;b\"0"), "a;b1");
    }
    #[test]
    fn scientific_and_fractions() {
        for (n, f, want) in [
            (12345., "0.00E+00", "1.23E+04"),
            (0.0012, "0.0E-00", "1.2E-03"),
            (12345., "##0.0E+0", "12.3E+3"),
            (1.5, "# ?/?", "1 1/2"),
            (0.125, "?/8", "1/8"),
            (2.75, "# ??/??", "2 3/4"),
        ] {
            assert_eq!(render(n, f), want, "{f}");
        }
    }
    #[test]
    fn date_systems_fake_leap_day_and_month_minute() {
        for (n, f, want) in [
            (1., "yyyy-mm-dd", "1900-01-01"),
            (59., "yyyy-mm-dd", "1900-02-28"),
            (60., "yyyy-mm-dd", "1900-02-29"),
            (61., "yyyy-mm-dd", "1900-03-01"),
            (25569., "yyyy-mm-dd", "1970-01-01"),
            (1.5, "mmm d, yyyy h:mm AM/PM", "Jan 1, 1900 12:00 PM"),
            (1.5, "[h]:mm:ss", "36:00:00"),
            (0.5, "[m]:ss", "720:00"),
            (0.5, "h:mm:ss", "12:00:00"),
        ] {
            assert_eq!(render(n, f), want, "{f}");
        }
        let f = parse_format("yyyy-mm-dd").unwrap();
        let options = FormatOptions {
            date_system: DateSystem::Excel1904,
            width: None,
        };
        assert_eq!(
            format_value(&CellValue::Number(0.), &f, options)
                .unwrap()
                .text,
            "1904-01-01"
        );
        assert_eq!(
            format_value(&CellValue::Number(24107.), &f, options)
                .unwrap()
                .text,
            "1970-01-01"
        );
    }
    #[test]
    fn padding_fill_and_typed_values() {
        assert_eq!(render(2., "0_)"), "2 ");
        let f = parse_format("*x0").unwrap();
        assert!(format_value(&CellValue::Number(2.), &f, FormatOptions::default()).is_err());
        let options = FormatOptions {
            width: Some(5),
            ..FormatOptions::default()
        };
        assert_eq!(
            format_value(&CellValue::Number(2.), &f, options)
                .unwrap()
                .text,
            "xxxx2"
        );
        let f = parse_format("0").unwrap();
        assert_eq!(
            format_value(&CellValue::Boolean(true), &f, options)
                .unwrap()
                .text,
            "TRUE"
        );
        assert_eq!(
            format_value(&CellValue::Blank, &f, options).unwrap().text,
            ""
        );
        assert_eq!(
            format_value(
                &CellValue::Error(crate::scalar::CellError::Div0),
                &f,
                options
            )
            .unwrap()
            .text,
            "#DIV/0!"
        );
    }
    #[test]
    fn malformed_and_unsupported_are_explicit() {
        for f in [
            "\"unterminated",
            "[Red",
            "0\\",
            "0_",
            "0*",
            "0;0;0;@;0",
            "[DBNum1]0",
            "[$-409]yyyy",
            "[>=]0",
        ] {
            assert!(parse_format(f).is_err(), "{f}");
        }
        let f = parse_format("0.0000000000000000").unwrap();
        assert!(format_value(&CellValue::Number(1.), &f, FormatOptions::default()).is_err());
        assert!(
            format_value(
                &CellValue::Number(f64::NAN),
                &parse_format("General").unwrap(),
                FormatOptions::default()
            )
            .is_err()
        );
    }
    #[test]
    fn public_tokens_are_bounded_and_elapsed_width_is_preserved() {
        assert_eq!(render(2. / 24., "[hh]:mm:ss"), "02:00:00");
        assert_eq!(render(2. / 1440., "[mm]:ss"), "02:00");
        assert_eq!(render(2. / 86400., "[ss]"), "02");
        let f = parse_format("0*x").unwrap();
        assert_eq!(
            format_value(
                &CellValue::Number(2.),
                &f,
                FormatOptions {
                    width: Some(5),
                    ..FormatOptions::default()
                }
            )
            .unwrap()
            .text,
            "2xxxx"
        );
        for tokens in [
            vec![Token::Decimal, Token::Digit('0')],
            vec![Token::Digit('0'), Token::Decimal],
            vec![
                Token::Exponent {
                    positive_sign: true,
                    digits: usize::MAX,
                },
                Token::Digit('0'),
            ],
            vec![Token::Date('x', usize::MAX)],
            vec![Token::Slash],
            vec![Token::Elapsed('h', usize::MAX)],
            vec![],
        ] {
            let f = Format {
                sections: vec![Section {
                    tokens,
                    condition: None,
                    color: None,
                }],
            };
            let _ = format_value(&CellValue::Number(2.), &f, FormatOptions::default());
        }
    }
    #[test]
    fn finite_input_never_formats_nonfinite_intermediates() {
        for (number, pattern) in [
            (f64::MAX, "0.0"),
            (f64::MAX, "0.000000000000000"),
            (f64::from_bits(1), "0.00E+00"),
            (f64::from_bits(2), "0.00E+00"),
        ] {
            let result = format_value(
                &CellValue::Number(number),
                &parse_format(pattern).unwrap(),
                FormatOptions::default(),
            );
            assert!(result.is_err(), "{number} {pattern}: {result:?}");
        }
        let wide = parse_format(&format!("{}.0E+00", "0".repeat(310))).unwrap();
        assert!(
            format_value(
                &CellValue::Number(f64::MAX),
                &wide,
                FormatOptions::default()
            )
            .is_err()
        );
        assert_eq!(render(1e100, "0.00E+00"), "1.00E+100");
        assert_eq!(render(1e-100, "0.00E+00"), "1.00E-100");
        for number in [f64::MAX, f64::MIN_POSITIVE, f64::from_bits(1), 0.] {
            let displayed = render(number, "General");
            assert!(!displayed.to_lowercase().contains("inf"));
            assert!(!displayed.to_lowercase().contains("nan"));
        }
    }
    #[test]
    fn bounded_generated_formats_never_panic() {
        let alphabet = [
            '0', '#', '?', '.', ',', '%', '@', 'm', 'h', 's', 'y', 'd', '[', ']', ';', '"', '\\',
            '_', '*', '😀',
        ];
        let mut seed = 9u64;
        for length in 0..64 {
            for _ in 0..20 {
                let mut input = String::new();
                for _ in 0..length {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    input.push(alphabet[seed as usize % alphabet.len()]);
                }
                if let Ok(f) = parse_format(&input) {
                    let _ = format_value(
                        &CellValue::Number(1234.25),
                        &f,
                        FormatOptions {
                            width: Some(20),
                            ..FormatOptions::default()
                        },
                    );
                }
            }
        }
    }
}
