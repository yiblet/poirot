//! Complex argument grammar and result spelling; independent of workbook and locale.
use crate::coercion::CellError;
use nom::Parser;
use std::sync::OnceLock;
type Result<T> = std::result::Result<T, CellError>;

/// Imaginary unit retained through parsing and formatting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComplexSuffix {
    /// Electrical/mathematical i spelling.
    I,
    /// Electrical j spelling.
    J,
}
impl ComplexSuffix {
    /// Canonical lowercase spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::I => "i",
            Self::J => "j",
        }
    }
}
/// Numeric complex components with the imaginary-unit spelling retained from input.
/// Engineering functions can calculate on the coefficients while preserving `i` or `j`
/// when formatting a result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComplexNumber {
    /// Real coefficient.
    pub real: f64,
    /// Imaginary coefficient.
    pub imaginary: f64,
    /// Imaginary unit (defaults to i for real-only input).
    pub suffix: ComplexSuffix,
}
/// Admit a suffix, retaining the established empty-string default.
pub fn coercev2_complex_suffix(text: &str) -> Result<ComplexSuffix> {
    match text {
        "" | "i" => Ok(ComplexSuffix::I),
        "j" => Ok(ComplexSuffix::J),
        _ => Err(CellError::Value),
    }
}
/// Complex component admission is identical for direct, referenced and selected
/// array scalars. Text capacity/work admission stays with the host; missing
/// arguments retain the host's capability failure rather than becoming blanks.
pub fn coercev2_complex_scalar(value: crate::scalar::Scalar) -> Result<Option<ComplexNumber>> {
    use crate::scalar::Scalar;
    match value {
        Scalar::Number(real) if real.is_finite() => Ok(Some(ComplexNumber {
            real,
            imaginary: 0.0,
            suffix: ComplexSuffix::I,
        })),
        Scalar::Blank => Ok(Some(ComplexNumber {
            real: 0.0,
            imaginary: 0.0,
            suffix: ComplexSuffix::I,
        })),
        Scalar::Text(text) => coercev2_complex_text(&text).map(Some),
        Scalar::Missing => Ok(None),
        Scalar::Error(error) => Err(error),
        Scalar::Boolean(_) => Err(CellError::Value),
        Scalar::Number(_) => Err(CellError::Num),
    }
}
/// Parse finite ASCII decimal/scientific coefficients; no whitespace, locale,
/// currency, percent, date, hex, infinity or NaN grammar is inherited.
pub fn coercev2_complex_text(text: &str) -> Result<ComplexNumber> {
    let (body, suffix, has_unit) = if let Some(body) = text.strip_suffix('i') {
        (body, ComplexSuffix::I, true)
    } else if let Some(body) = text.strip_suffix('j') {
        (body, ComplexSuffix::J, true)
    } else {
        (text, ComplexSuffix::I, false)
    };
    let mut real = 0.0;
    let imaginary;
    if has_unit {
        // A sign separates coefficients only when it is not an exponent sign.
        let mut previous = None;
        let mut split = None;
        for (at, byte) in body.bytes().enumerate() {
            if at != 0
                && matches!(byte, b'+' | b'-')
                && !matches!(previous, Some(b'e' | b'E'))
                && split.replace(at).is_some()
            {
                return Err(CellError::Num);
            }
            previous = Some(byte);
        }
        let coefficient = if let Some(at) = split {
            let r = body.get(..at).ok_or(CellError::Num)?;
            real = coercev2_complex_coefficient(r)?;
            body.get(at..).ok_or(CellError::Num)?
        } else {
            body
        };
        imaginary = match coefficient {
            "" | "+" => 1.0,
            "-" => -1.0,
            other => coercev2_complex_coefficient(other)?,
        };
    } else {
        real = coercev2_complex_coefficient(body)?;
        imaginary = 0.0;
    }
    Ok(ComplexNumber {
        real,
        imaginary,
        suffix,
    })
}
fn coercev2_complex_coefficient(text: &str) -> Result<f64> {
    // nom's decimal/scientific number parser consumes no surrounding whitespace;
    // all_consuming rejects locale separators, suffixes and trailing operators.
    let (_, n) = nom::combinator::all_consuming(
        nom::number::complete::double::<&str, nom::error::Error<&str>>,
    )
    .parse(text)
    .map_err(|_| CellError::Num)?;
    if n.is_finite() {
        Ok(n)
    } else {
        Err(CellError::Num)
    }
}
/// Render computed coefficients using the host's established number formatter.
/// Zero is canonical text zero; negative unit coefficients retain their sign.
// TODO(coercion-non-argument): [LOW] Computed result spelling can round component digits.
pub fn coerce_complex_result(
    value: ComplexNumber,
    format_number: impl Fn(f64) -> String,
) -> Result<String> {
    if !value.real.is_finite() || !value.imaginary.is_finite() {
        return Err(CellError::Num);
    }
    let mut text = String::new();
    if value.real != 0.0 {
        // TODO(coercion-non-argument): [LOW] Spreadsheet precision can discard computed component digits.
        text.push_str(&format_number(value.real));
    }
    if value.imaginary != 0.0 {
        if value.imaginary == -1.0 {
            text.push('-');
        } else {
            if !text.is_empty() && value.imaginary > 0.0 {
                text.push('+');
            }
            if value.imaginary != 1.0 {
                // TODO(coercion-non-argument): [LOW] Spreadsheet precision can discard computed component digits.
                text.push_str(&format_number(value.imaginary));
            }
        }
        text.push_str(value.suffix.as_str());
    }
    if text.is_empty() {
        text.push('0');
    }
    Ok(text)
}

/// The formatter is supplied by the adapter to preserve the engine's Excel number spelling.
// TODO(coercion-non-argument): [LOW] Pinned result spelling intentionally retains historical sign and zero behavior.
pub fn coerce_complex_pinned_result(
    real: f64,
    imaginary: f64,
    suffix: &str,
    format_number: impl Fn(f64) -> String,
) -> Result<String> {
    let suffix = if suffix.is_empty() { "i" } else { suffix };
    if suffix != "i" && suffix != "j" {
        return Err(CellError::Value);
    }
    let mut text = String::new();
    if real != 0.0 {
        // TODO(coercion-non-argument): [LOW] Pinned result spelling rounds computed components.
        text.push_str(&format_number(real));
    }
    if imaginary != 0.0 {
        if !text.is_empty() && imaginary > 0.0 {
            text.push('+');
        }
        if imaginary != 1.0 && imaginary != -1.0 {
            // TODO(coercion-non-argument): [LOW] Pinned result spelling rounds computed components.
            text.push_str(&format_number(imaginary));
        }
        text.push_str(suffix);
    }
    Ok(text)
}

fn coercev2_complex_pinned_parts(text: &str) -> Result<regex::Captures<'_>> {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN
        .get_or_init(|| {
            // SAFETY: The literal pattern below has balanced groups/classes and uses only supported regex syntax, independent of text.
            regex::Regex::new(
                r"\A([+-]?)([0-9]+\.[0-9]+|[0-9]*)([+-]?)([0-9]+\.[0-9]+|[0-9]*)([ij]?)\z",
            )
            .expect("constant regex")
        })
        .captures(text)
        .ok_or(CellError::Num)
}
/// Extract the imaginary component of a complex-number spelling.
pub fn coercev2_complex_pinned_imaginary(text: &str) -> Result<String> {
    let parts = coercev2_complex_pinned_parts(text)?;
    if parts.get(5).ok_or(CellError::Num)?.as_str().is_empty() {
        return Ok("0".into());
    }
    let sign = if parts.get(3).ok_or(CellError::Num)?.as_str() == "-" {
        "-"
    } else {
        ""
    };
    Ok(format!(
        "{sign}{}",
        if parts.get(4).ok_or(CellError::Num)?.as_str().is_empty() {
            "1"
        } else {
            parts.get(4).ok_or(CellError::Num)?.as_str()
        }
    ))
}
/// Extract the real component of a complex-number spelling.
pub fn coercev2_complex_pinned_real(text: &str) -> Result<String> {
    let parts = coercev2_complex_pinned_parts(text)?;
    if parts.get(2).ok_or(CellError::Num)?.as_str().is_empty() {
        return Ok("0".into());
    }
    Ok(format!(
        "{}{}",
        if parts.get(1).ok_or(CellError::Num)?.as_str() == "+" {
            ""
        } else {
            parts.get(1).ok_or(CellError::Num)?.as_str()
        },
        parts.get(2).ok_or(CellError::Num)?.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_components_preserve_sign_and_result_type() {
        // Microsoft COMPLEX/IMREAL/IMAGINARY documentation, corroborated by
        // the independent complex boundary Calc report for valid spellings.
        for (text, real, imaginary, suffix) in [
            ("5", 5.0, 0.0, ComplexSuffix::I),
            ("i", 0.0, 1.0, ComplexSuffix::I),
            ("-j", 0.0, -1.0, ComplexSuffix::J),
            ("+i", 0.0, 1.0, ComplexSuffix::I),
            ("2-i", 2.0, -1.0, ComplexSuffix::I),
            ("-2+3j", -2.0, 3.0, ComplexSuffix::J),
            ("-2i", 0.0, -2.0, ComplexSuffix::I),
            ("1e-2-3e+2j", 0.01, -300.0, ComplexSuffix::J),
            (".5i", 0.0, 0.5, ComplexSuffix::I),
            ("2.", 2.0, 0.0, ComplexSuffix::I),
            ("-0-0i", 0.0, 0.0, ComplexSuffix::I),
        ] {
            assert_eq!(
                coercev2_complex_text(text),
                Ok(ComplexNumber {
                    real,
                    imaginary,
                    suffix
                }),
                "{text}"
            );
        }
        for (real, imaginary, expected) in [
            (0.0, 0.0, "0"),
            (-0.0, -0.0, "0"),
            (3.0, -1.0, "3-i"),
            (0.0, -1.0, "-i"),
            (0.0, 1.0, "i"),
            (3.0, 2.0, "3+2i"),
            (-3.0, -2.0, "-3-2i"),
            (3.0, 0.0, "3"),
        ] {
            assert_eq!(
                coerce_complex_result(
                    ComplexNumber {
                        real,
                        imaginary,
                        suffix: ComplexSuffix::I
                    },
                    |n| n.to_string()
                ),
                Ok(expected.into())
            );
        }
    }

    #[test]
    fn complex_grammar_does_not_inherit_general_number_or_date_conversion() {
        // Invalid spellings are rejected. Calc uses #VALUE! for many invalid
        // spellings and accepts decimal comma; Poirot retains its #NUM! domain
        // and deliberately does not enable locale-dependent parsing here.
        for text in [
            "", "+", "-", "1-2", "1++2i", "1+-2j", "1e-i", "1e2e3i", "2+3I", "2+3k", "2ij",
            " 2+3i", "2 +3i", "2+3i ", "NaN", "inf", "1e309i", "0x1i", "$2", "2%", "1,000",
            "1/2/2020", "２i", "😀i",
        ] {
            assert_eq!(coercev2_complex_text(text), Err(CellError::Num), "{text}");
        }
        use crate::scalar::Scalar;
        assert_eq!(
            coercev2_complex_scalar(Scalar::Boolean(true)),
            Err(CellError::Value)
        );
        assert_eq!(
            coercev2_complex_scalar(Scalar::Error(CellError::NA)),
            Err(CellError::NA)
        );
        assert_eq!(coercev2_complex_scalar(Scalar::Missing), Ok(None));
        let exact = 1.2345678901234567;
        assert_eq!(
            coercev2_complex_scalar(Scalar::Number(exact))
                .unwrap()
                .unwrap()
                .real
                .to_bits(),
            exact.to_bits()
        );
        assert_eq!(
            coerce_complex_result(
                ComplexNumber {
                    real: f64::INFINITY,
                    imaginary: 0.0,
                    suffix: ComplexSuffix::I
                },
                |n| n.to_string()
            ),
            Err(CellError::Num)
        );
    }
}
