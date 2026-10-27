//! Compile and execute bounded rational layouts with ordered denominator search.

use super::{FractionDenominator, FractionPlan};
use crate::format::{FormatError, RenderedText, Token, error};

const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_992.;
fn exact_integer(n: f64) -> Result<u64, FormatError> {
    if !n.is_finite() || !(0. ..=MAX_EXACT_INTEGER).contains(&n) || n.fract() != 0. {
        return Err(error(
            crate::format::FormatReasonError::FractionIntegerExceedsSupportedExactRange,
        ));
    }
    Ok(n as u64)
}
fn fraction_literals(tokens: &[Token]) -> Result<RenderedText, FormatError> {
    if tokens
        .iter()
        .any(|t| !matches!(t, Token::Literal(_) | Token::Padding(_) | Token::Fill(_)))
    {
        return Err(error(
            crate::format::FormatReasonError::UnsupportedTokensInFractionLiteralRegion,
        ));
    }
    Ok(RenderedText::literals(tokens))
}
impl FractionPlan {
    /// Validate fraction geometry and denominator bounds before rendering.
    pub(super) fn compile_fraction(tokens: &[Token]) -> Result<Self, FormatError> {
        if tokens
            .iter()
            .any(|t| matches!(t, Token::Percent | Token::Comma))
        {
            return Err(error(
                crate::format::FormatReasonError::FractionPercentCommaScalingIsUnsupported,
            ));
        }
        let slashes: Vec<_> = tokens
            .iter()
            .enumerate()
            .filter_map(|(i, t)| matches!(t, Token::Slash).then_some(i))
            .collect();
        if slashes.len() != 1 {
            return Err(error(
                crate::format::FormatReasonError::FractionRequiresOneSlash,
            ));
        }
        // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
        let slash = slashes[0];
        let numerator_start = (0..slash)
            .rev()
            // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
            .take_while(|i| matches!(tokens[*i], Token::Digit(_)))
            .last()
            .ok_or_else(|| error(crate::format::FormatReasonError::FractionHasNoNumerator))?;
        let numerator_width = slash - numerator_start;
        if numerator_width > 15 {
            return Err(error(
                crate::format::FormatReasonError::FractionNumeratorWidthExceedsLimit,
            ));
        }
        let denom_end = (slash + 1..tokens.len()).take_while(|i| {
            // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
            matches!(tokens[*i], Token::Digit(_))
                // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
                || matches!(&tokens[*i], Token::Literal(s) if !s.is_empty() && s.chars().all(|c|c.is_ascii_digit()))
        }).last().map_or(slash + 1, |i| i + 1);
        // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
        let dt = &tokens[slash + 1..denom_end];
        let (denominator, denominator_width) = if dt.iter().any(|t| matches!(t, Token::Literal(_)))
        {
            let text = dt
                .iter()
                .map(|t| match t {
                    Token::Digit('0') => "0".into(),
                    Token::Literal(s) => s.clone(),
                    _ => String::new(),
                })
                .collect::<String>();
            if dt.iter().any(|t| matches!(t, Token::Digit(c) if *c != '0')) {
                return Err(error(
                    crate::format::FormatReasonError::MixedFixedVariableFractionDenominator,
                ));
            }
            let d = text.parse::<u32>().map_err(|_| {
                error(crate::format::FormatReasonError::FixedFractionDenominatorExceedsU32Range)
            })?;
            if d == 0 {
                return Err(error(
                    crate::format::FormatReasonError::ZeroFractionDenominator,
                ));
            }
            (FractionDenominator::Fixed(d), text.len())
        } else {
            if dt.is_empty() || dt.len() > 4 {
                return Err(error(crate::format::FormatReasonError::FractionDenominatorRequiresOneToFourPlaceholders));
            }
            (
                // SAFETY: The local guard restricts dt.len() to 1..=4; 10^4 fits u64 and the result is positive.
                FractionDenominator::Variable(10u64.pow(dt.len() as u32) - 1),
                dt.len(),
            )
        };
        // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
        let before = &tokens[..numerator_start];
        let first_whole = before.iter().position(|t| matches!(t, Token::Digit(_)));
        let (prefix, separator, required_whole) = if let Some(first) = first_whole {
            // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
            let end = (first..before.len())
                // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
                .take_while(|i| matches!(before[*i], Token::Digit(_)))
                .last()
                .unwrap()
                + 1;
            if end - first > 15 {
                return Err(error(
                    crate::format::FormatReasonError::FractionWholeWidthExceedsLimit,
                ));
            }
            (
                // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
                fraction_literals(&before[..first])?,
                // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
                fraction_literals(&before[end..])?,
                // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
                before[first..end]
                    .iter()
                    .any(|t| matches!(t, Token::Digit('0'))),
            )
        } else {
            (fraction_literals(before)?, RenderedText::default(), false)
        };
        Ok(Self {
            compact: false,
            numerator_width,
            denominator_width,
            denominator,
            mixed: first_whole.is_some(),
            required_whole,
            prefix,
            separator,
            // SAFETY: The slash count is one; all positions are selected within tokens, and first/end delimit a nonempty digit run before the numerator.
            suffix: fraction_literals(&tokens[denom_end..])?,
        })
    }
    /// Render an admitted fraction layout and preserve literal spans.
    pub(super) fn render_fraction_output(&self, n: f64) -> Result<RenderedText, FormatError> {
        if !n.is_finite() || n.abs() > MAX_EXACT_INTEGER {
            return Err(error(
                crate::format::FormatReasonError::FractionMagnitudeExceedsExactIntegerRange,
            ));
        }
        let magnitude = n.abs();
        let mut whole = if self.mixed {
            exact_integer(magnitude.floor())?
        } else {
            0
        };
        let fractional = magnitude - whole as f64;
        let (mut numerator, denominator) = match self.denominator {
            FractionDenominator::Fixed(d) => (
                exact_integer((fractional * f64::from(d)).round())?,
                u64::from(d),
            ),
            FractionDenominator::Variable(max) => {
                let (mut best, mut delta) = ((0, 1), f64::INFINITY);
                for d in 1..=max {
                    let num = exact_integer((fractional * d as f64).round())?;
                    let diff = (fractional - num as f64 / d as f64).abs();
                    if diff < delta {
                        best = (num, d);
                        delta = diff;
                    }
                }
                best
            }
        };
        if self.mixed && numerator >= denominator {
            whole = whole
                .checked_add(numerator / denominator)
                .ok_or_else(|| error(crate::format::FormatReasonError::FractionWholeOverflow))?;
            if whole > MAX_EXACT_INTEGER as u64 {
                return Err(error(
                    crate::format::FormatReasonError::FractionWholeExceedsExactRange,
                ));
            }
            numerator %= denominator;
        }
        let sign = if n < 0. { "-" } else { "" };
        let whole_text = if self.mixed && (whole != 0 || self.required_whole || numerator == 0) {
            whole.to_string()
        } else {
            String::new()
        };
        let body = if self.mixed && numerator == 0 {
            if self.compact {
                String::new()
            } else {
                let width = self
                    .numerator_width
                    .checked_add(1)
                    .and_then(|w| w.checked_add(self.denominator_width))
                    .filter(|w| *w <= 32767)
                    .ok_or_else(|| {
                        error(crate::format::FormatReasonError::FractionPaddingExceedsCellLimit)
                    })?;
                // SAFETY: The checked padding width is at most 32767; each repeated space is one byte.
                " ".repeat(width)
            }
        } else if self.compact {
            format!("{numerator}/{denominator}")
        } else {
            format!(
                "{numerator:>nw$}/{denominator:>dw$}",
                nw = self.numerator_width,
                dw = self.denominator_width
            )
        };
        let mut out = RenderedText::plain(sign);
        out.append(&self.prefix);
        out.push_str(&whole_text);
        if !self.compact || (!whole_text.is_empty() && !body.is_empty()) {
            out.append(&self.separator);
        }
        out.push_str(&body);
        out.append(&self.suffix);
        Ok(out)
    }
}

#[cfg(test)]
mod fraction_tests {
    use super::{FractionPlan, MAX_EXACT_INTEGER, exact_integer};
    use crate::format::{FormatError, Token, parse_format};
    impl FractionPlan {
        fn render(&self, n: f64) -> Result<String, FormatError> {
            self.render_output(n)?.finish(None, 32767)
        }
    }
    fn plan(code: &str) -> FractionPlan {
        FractionPlan::compile(&parse_format(code).unwrap().sections[0].tokens).unwrap()
    }
    #[test]
    fn fraction_plans_reuse_layout_and_preserve_affixes_when_zero_or_whole() {
        let p = plan("\"USD \"# ?/?\" kg\"");
        assert_eq!(p.render(1.5).unwrap(), "USD 1 1/2 kg");
        assert_eq!(p.render(1.01).unwrap(), "USD 1     kg");
        assert_eq!(p.render(-1.01).unwrap(), "-USD 1     kg");
        assert_eq!(p.render(0.).unwrap(), "USD 0     kg");
        let fixed = plan("\"~\"# ?/8\" oz\"");
        assert_eq!(fixed.render(0.99).unwrap(), "~1     oz");
        assert_eq!(fixed.render(0.125).unwrap(), "~ 1/8 oz");
        assert_eq!(plan("??/??").render(2.75).unwrap(), "11/ 4");
        // Fixed denominator rounding retains denominator, not a reduced fraction.
        for numerator in 0..=8 {
            assert_eq!(
                plan("?/8").render(numerator as f64 / 8.).unwrap(),
                format!("{numerator}/8")
            );
        }
        let prefix = p.prefix.text.as_ptr();
        p.render(2.25).unwrap();
        assert_eq!(p.prefix.text.as_ptr(), prefix);
    }
    #[test]
    fn fraction_integer_conversions_are_finite_bounded_and_exact() {
        for n in [
            f64::NAN,
            f64::INFINITY,
            -1.,
            1.5,
            MAX_EXACT_INTEGER + 2.,
            f64::MAX,
        ] {
            assert!(exact_integer(n).is_err(), "{n}");
        }
        assert_eq!(
            exact_integer(MAX_EXACT_INTEGER).unwrap(),
            9_007_199_254_740_992
        );
        let fixed = plan("?/4294967295");
        assert!(fixed.render(MAX_EXACT_INTEGER).is_err());
        assert!(fixed.render(f64::MAX).is_err());
        assert!(fixed.render(f64::NAN).is_err());
        assert_eq!(
            plan("?/1").render(MAX_EXACT_INTEGER).unwrap(),
            "9007199254740992/1"
        );
        // Mixed magnitude is bounded independently from a safe small fractional numerator.
        assert!(plan("# ?/8").render(MAX_EXACT_INTEGER + 2.).is_err());
        assert!(plan("?/????").render(MAX_EXACT_INTEGER).is_err());
    }
    #[test]
    fn fraction_compilation_rejects_ignored_scaling_and_invalid_layouts() {
        for code in [
            "?/?%",
            "# ?/?,",
            "#,##0 ?/?",
            "?/4294967296",
            "?/8?",
            "?/?/?",
            "?/?????",
            "?.?/?",
        ] {
            let f = parse_format(code).unwrap();
            assert!(
                FractionPlan::compile(&f.sections[0].tokens).is_err(),
                "{code}"
            );
        }
        assert!(
            FractionPlan::compile(&[Token::Digit('?'), Token::Slash, Token::Literal("0".into())])
                .is_err()
        );
        assert!(FractionPlan::compile(&[Token::Digit('?')]).is_err());
        // Quoted percent/comma remain literal affixes, not scaling directives.
        assert_eq!(plan("?/?\"%,\"").render(0.25).unwrap(), "1/4%,");
    }
}
