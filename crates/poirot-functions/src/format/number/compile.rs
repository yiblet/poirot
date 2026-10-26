//! Compile numeric tokens into an immutable placeholder layout.

use super::{GeneralPart, NumberPlan, NumberProgram, NumericPlan};
use crate::format::{FormatError, RenderedText, Token, error};

impl NumberPlan {
    /// Resolve numeric tokens into checked digit, scaling, and affix state.
    pub(super) fn compile_numeric(tokens: &[Token]) -> Result<Self, FormatError> {
        {
            let general = tokens.iter().any(|t| matches!(t, Token::General));
            let digits = tokens.iter().any(|t| matches!(t, Token::Digit(_)));
            let exponent = tokens.iter().any(|t| matches!(t, Token::Exponent { .. }));
            if (general
                && (digits
                    || exponent
                    || tokens.iter().any(|t| {
                        matches!(
                            t,
                            Token::Decimal
                                | Token::Comma
                                | Token::Percent
                                | Token::Text
                                | Token::Slash
                        )
                    })))
                || (exponent && !digits)
                || (!general && digits && tokens.iter().any(|t| matches!(t, Token::Text)))
            {
                return Err(error("unsupported tokens in compiled numeric section"));
            }
        }
        if tokens.iter().any(|t| matches!(t, Token::General)) {
            return Ok(Self {
                program: NumberProgram::General(
                    tokens
                        .iter()
                        .map(|t| {
                            if matches!(t, Token::General) {
                                GeneralPart::Value
                            } else {
                                GeneralPart::Literal(RenderedText::literals(std::slice::from_ref(
                                    t,
                                )))
                            }
                        })
                        .collect(),
                ),
            });
        }
        let positions: Vec<_> = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, t)| matches!(t, Token::Digit(_)).then_some(i))
            .collect();
        if positions.is_empty() {
            return Ok(Self {
                program: NumberProgram::Literal(RenderedText::literals(tokens)),
            });
        }
        // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
        let first = positions[0];
        // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
        let last = *positions.last().unwrap();
        let decimal = tokens.iter().position(|t| matches!(t, Token::Decimal));
        let exponent = tokens
            .iter()
            .position(|t| matches!(t, Token::Exponent { .. }));
        if exponent.is_some_and(|e| e < last)
            || decimal.is_some_and(|d| {
                tokens
                    .get(d + 1..last + 1)
                    .unwrap_or(&[])
                    .iter()
                    .any(|t| matches!(t, Token::Comma))
            })
        {
            return Err(error("unsupported numeric placeholder placement"));
        }
        let int_end = decimal.or(exponent).unwrap_or(last + 1);
        let numeric_start = first.min(decimal.unwrap_or(first));
        let int_tokens = tokens.get(first..int_end).unwrap_or(&[]);
        if tokens
            .iter()
            .filter(|t| matches!(t, Token::Decimal))
            .count()
            > 1
            || tokens
                .iter()
                .filter(|t| matches!(t, Token::Exponent { .. }))
                .count()
                > 1
            || exponent.is_some_and(|e| e < first || decimal.is_some_and(|d| d > e))
        {
            return Err(error("unsupported numeric token order"));
        }
        let numeric_tokens = tokens.get(numeric_start..=last).unwrap_or(&[]);
        let embedded_literals = numeric_tokens
            .iter()
            .any(|t| matches!(t, Token::Literal(_)));
        let mut integer_literals = Vec::new();
        if embedded_literals {
            // Only integer slots and literal runs have an unambiguous mask layout.
            if tokens
                .iter()
                .any(|t| !matches!(t, Token::Digit(_) | Token::Literal(_)))
            {
                return Err(error("integer masks require only digits and literals"));
            }
            let mut literal = String::new();
            for token in numeric_tokens {
                match token {
                    Token::Literal(text) => literal.push_str(text),
                    Token::Digit(_) => integer_literals.push(std::mem::take(&mut literal)),
                    _ => {}
                }
            }
        } else if numeric_tokens.iter().any(|t| {
            matches!(
                t,
                Token::Padding(_) | Token::Fill(_) | Token::Text | Token::Percent
            )
        }) {
            return Err(error(
                "embedded directives among numeric placeholders unsupported",
            ));
        }
        let decimals: Vec<_> = if let Some(d) = decimal {
            tokens
                .get(d + 1..exponent.unwrap_or(last + 1))
                .unwrap_or(&[])
                .iter()
                .filter_map(|t| {
                    if let Token::Digit(c) = t {
                        Some(*c)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            vec![]
        };
        if decimals.len() > 15 {
            return Err(error("more than fifteen decimal placeholders unsupported"));
        }
        let mut scaling = 0;
        let mut i = (last + 1).max(decimal.map_or(0, |d| d + 1));
        // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
        while i < tokens.len() && matches!(tokens[i], Token::Comma) {
            scaling += 1;
            i += 1
        }
        if let Some(d) = decimal {
            let mut p = d;
            // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
            while p > first && matches!(tokens[p - 1], Token::Comma) {
                scaling += 1;
                p -= 1
            }
        }
        let percentages = tokens
            .iter()
            .filter(|t| matches!(t, Token::Percent))
            .count();
        if percentages > 6 || scaling > 100 {
            return Err(error("scaling exceeds resource limit"));
        }
        let integer: Vec<_> = int_tokens
            .iter()
            .filter_map(|t| {
                if let Token::Digit(c) = t {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect();
        let grouping = int_tokens.iter().any(|t| matches!(t, Token::Comma))
            && int_tokens
                .iter()
                .rev()
                .skip_while(|t| matches!(t, Token::Comma))
                .any(|t| matches!(t, Token::Comma));
        // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
        let exp = exponent.map(|e| match tokens[e] {
            Token::Exponent {
                positive_sign,
                digits,
            } => (positive_sign, digits),
            // SAFETY: exponent was found by matching Token::Exponent in this same immutable tokens slice.
            _ => unreachable!(),
        });
        if exp.is_some_and(|(_, digits)| digits > 15) {
            return Err(error("exponent width exceeds resource limit"));
        }
        let suffix = if let Some(e) = exponent {
            // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
            RenderedText::literals(&tokens[e + 1..])
        } else {
            // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
            RenderedText::literals(&tokens[i..])
        };
        Ok(Self {
            program: NumberProgram::Numeric(NumericPlan {
                minimum_visible_digit: false,
                min_integer: integer.iter().filter(|c| **c == '0').count(),
                question_count: integer.iter().filter(|c| **c == '?').count(),
                integer,
                integer_literals,
                decimal: decimals,
                has_decimal: decimal.is_some(),
                grouping,
                scaling,
                percentages: percentages as i32,
                exponent: exp,
                // SAFETY: positions is nonempty here; first/last and exponent/decimal positions come from this tokens slice, numeric_start <= last, and both scan cursors are bounded locally.
                prefix: RenderedText::literals(&tokens[..numeric_start]),
                suffix,
            }),
        })
    }
}
