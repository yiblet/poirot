//! Execute immutable numeric layouts and their rounding and text kernels.

use super::{GeneralPart, NumberPlan, NumberProgram};
use crate::format::{FormatError, RenderedText, error};

fn approx(n: f64) -> f64 {
    format!("{n:.14e}").parse().unwrap_or(n)
}
fn rounded(n: f64, places: usize) -> Result<f64, FormatError> {
    let factor = 10f64.powi(places as i32);
    let scaled = approx(n * factor);
    if !scaled.is_finite() {
        return Err(error(
            crate::format::FormatReasonError::NumberRoundingExceedsSupportedFiniteRange,
        ));
    }
    let rounded = scaled.round() / factor;
    if !rounded.is_finite() {
        return Err(error(
            crate::format::FormatReasonError::NumberRoundingExceedsSupportedFiniteRange,
        ));
    }
    Ok(rounded)
}

impl NumberPlan {
    /// Render the resolved numeric layout under output resource limits.
    pub(super) fn render_numeric_output(&self, n: f64) -> Result<RenderedText, FormatError> {
        if !n.is_finite() {
            return Err(error(crate::format::FormatReasonError::NonFiniteNumber));
        }
        let p = match &self.program {
            NumberProgram::General(parts) => {
                let mut out = RenderedText::default();
                for part in parts {
                    match part {
                        GeneralPart::Value => out.push_str(&general(n)),
                        GeneralPart::Literal(text) => out.append(text),
                    }
                }
                return Ok(out);
            }
            NumberProgram::Literal(s) => return Ok(s.clone()),
            NumberProgram::Numeric(p) => p,
        };
        let value = n * 100f64.powi(p.percentages) / 1000f64.powi(p.scaling);
        if !value.is_finite() {
            return Err(error(
                crate::format::FormatReasonError::ScaledNumberOverflow,
            ));
        }
        let negative = value.is_sign_negative() && value != 0.;
        let value = value.abs();
        let mut exponent_text = String::new();
        let value = if let Some((positive_sign, digits)) = p.exponent {
            let integer_count = p.integer.len().max(1);
            let mut exp = if value == 0. {
                0
            } else {
                (value.log10().floor() as i32).div_euclid(integer_count as i32)
                    * integer_count as i32
            };
            let mut mantissa = value / 10f64.powi(exp);
            if !mantissa.is_finite() {
                return Err(error(
                    crate::format::FormatReasonError::ScientificExponentExceedsSupportedFiniteRange,
                ));
            }
            if rounded(mantissa, p.decimal.len())? >= 10f64.powi(integer_count as i32) {
                exp += integer_count as i32;
                mantissa = value / 10f64.powi(exp);
                if !mantissa.is_finite() {
                    return Err(error(crate::format::FormatReasonError::ScientificExponentExceedsSupportedFiniteRange));
                }
            }
            exponent_text = format!(
                "E{}{:0digits$}",
                if exp < 0 {
                    "-"
                } else if positive_sign {
                    "+"
                } else {
                    ""
                },
                exp.unsigned_abs()
            );
            mantissa
        } else {
            value
        };
        let number = format!("{:.*}", p.decimal.len(), rounded(value, p.decimal.len())?);
        let (mut integer, fraction) = number
            .split_once('.')
            .map_or((number.as_str(), ""), |(a, b)| (a, b));
        if integer == "0" && (p.min_integer == 0 || !p.integer_literals.is_empty()) {
            integer = "";
        }
        if !p.integer_literals.is_empty() {
            // Fixed notation produces ASCII digits, so byte and slot counts agree.
            let missing = p.integer.len().saturating_sub(integer.len());
            let extra = integer.len().saturating_sub(p.integer.len());
            let mut digits = integer.chars();
            let mut out = RenderedText::default();
            if negative {
                out.push_str("-");
            }
            out.append(&p.prefix);
            for digit in digits.by_ref().take(extra) {
                // SAFETY: The local four-byte buffer fits the UTF-8 encoding of every char.
                out.push_str(digit.encode_utf8(&mut [0; 4]));
            }
            for (index, (placeholder, literal)) in
                p.integer.iter().zip(&p.integer_literals).enumerate()
            {
                out.push_str(literal);
                let digit = if index >= missing {
                    digits.next()
                } else {
                    match placeholder {
                        '0' => Some('0'),
                        '?' => Some(' '),
                        _ => None,
                    }
                };
                if let Some(digit) = digit {
                    // SAFETY: The local four-byte buffer fits the UTF-8 encoding of every char.
                    out.push_str(digit.encode_utf8(&mut [0; 4]));
                }
            }
            out.append(&p.suffix);
            return Ok(out);
        }
        let mut integer = format!("{:0>width$}", integer, width = p.min_integer);
        if p.question_count > 0 && integer.len() < p.integer.len() {
            integer = format!(
                "{}{}",
                // SAFETY: The branch checks integer.len() < p.integer.len(); the repeat count is bounded by that existing vector's length and each space is one byte.
                " ".repeat((p.integer.len() - integer.len()).min(p.question_count)),
                integer
            );
        }
        if p.grouping {
            integer = group(&integer);
        }
        let mut fraction = fraction.to_owned();
        for index in (0..p.decimal.len()).rev() {
            if fraction.as_bytes().get(index) != Some(&b'0') {
                break;
            }
            // SAFETY: index comes from 0..p.decimal.len(), and p.decimal is not resized during this loop.
            match p.decimal[index] {
                '#' => {
                    fraction.pop();
                }
                // SAFETY: The preceding byte check found ASCII 0 at index, proving index and index + 1 are in-bounds UTF-8 boundaries.
                '?' => fraction.replace_range(index..index + 1, " "),
                _ => break,
            }
        }
        if p.minimum_visible_digit && integer.is_empty() && fraction.is_empty() {
            integer.push('0');
        }
        let decimal_text = if p.has_decimal && (!fraction.is_empty() || p.decimal.is_empty()) {
            format!(".{fraction}")
        } else {
            String::new()
        };
        // POI DataFormatter delegates numeric patterns to Java DecimalFormat.
        // Its omitted negative subpattern adds a minus before the whole positive
        // prefix. Explicit negative sections arrive here with absolute magnitude.
        let mut out = RenderedText::default();
        if negative {
            out.push_str("-");
        }
        out.append(&p.prefix);
        out.push_str(&format!("{integer}{decimal_text}{exponent_text}"));
        out.append(&p.suffix);
        Ok(out)
    }
}

fn group(s: &str) -> String {
    let chars: Vec<_> = s.chars().collect();
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i > 0 && (chars.len() - i) % 3 == 0 && *c != ' ' {
                format!(",{c}")
            } else {
                c.to_string()
            }
        })
        .collect()
}
fn general(n: f64) -> String {
    if n == 0. {
        return "0".into();
    }
    let a = n.abs();
    if !(1e-9..1e11).contains(&a) {
        let s = format!("{n:.10e}");
        let Some((m, e)) = s.split_once('e') else {
            return s;
        };
        let m = m.trim_end_matches('0').trim_end_matches('.');
        let Ok(e) = e.parse::<i32>() else {
            return s;
        };
        format!("{m}E{}{e:02}", if e >= 0 { "+" } else { "" })
    } else {
        approx(n).to_string()
    }
}
