//! Validate parsed format tokens and select a renderer for each section.
use super::fingerprint::program_fingerprint;
use super::{
    Format, FormatCompileError, FormatCompilePolicy, FormatError, FormatPlan, NumberPlan,
    SectionKind, SectionPlan, SectionProgram, Token, date, error, is_text_section, number,
    parse_format,
};

fn diagnostic(section: usize, token: Option<usize>, source: FormatError) -> FormatCompileError {
    FormatCompileError {
        section: Some(section),
        token,
        source,
    }
}

/// Compile an Excel number-format string for repeated rendering.
pub fn compile_format(
    input: &str,
    policy: FormatCompilePolicy,
) -> Result<FormatPlan, FormatCompileError> {
    compile_program(&parse_format(input)?, policy)
}
/// Compile an already parsed number format, including caller-constructed tokens.
pub fn compile_program(
    format: &Format,
    policy: FormatCompilePolicy,
) -> Result<FormatPlan, FormatCompileError> {
    if format.sections.is_empty() {
        return Err(error(crate::format::FormatReasonError::FormatHasNoSections).into());
    }
    if format.sections.len() > 4 {
        return Err(error(crate::format::FormatReasonError::TooManySections).into());
    }
    let mut sections = Vec::with_capacity(format.sections.len());
    for (index, section) in format.sections.iter().enumerate() {
        if section.tokens.len() > policy.max_tokens_per_section.min(32768) {
            return Err(diagnostic(
                index,
                None,
                error(crate::format::FormatReasonError::TokenLimitExceeded),
            ));
        }
        for (i, token) in section.tokens.iter().enumerate() {
            let invalid = match token {
                Token::Elapsed(c, w) if !matches!(c, 'h' | 'm' | 's') || !(1..=2).contains(w) => {
                    Some(crate::format::FormatReasonError::UnsupportedElapsedToken)
                }
                Token::Exponent { digits, .. } if !(1..=15).contains(digits) => {
                    Some(crate::format::FormatReasonError::UnsupportedExponentWidth)
                }
                Token::Digit(c) if !matches!(c, '0' | '#' | '?') => {
                    Some(crate::format::FormatReasonError::UnsupportedDigitPlaceholder)
                }
                Token::Literal(s)
                    if s.chars().count() > policy.max_literal_characters.min(32767) =>
                {
                    Some(crate::format::FormatReasonError::LiteralExceedsCellLimit)
                }
                _ => None,
            };
            if let Some(message) = invalid {
                return Err(diagnostic(index, Some(i), error(message)));
            }
        }
        let dates = section.tokens.iter().any(|t| {
            matches!(
                t,
                Token::Date(..) | Token::Elapsed(..) | Token::AmPm(_) | Token::SystemDateTime(_)
            )
        });
        let fraction = !dates
            && !section.tokens.iter().any(|t| matches!(t, Token::General))
            && section.tokens.iter().any(|t| matches!(t, Token::Digit(_)))
            && section.tokens.iter().any(|t| matches!(t, Token::Slash));
        let (kind, program) = if dates {
            match date::DatePlan::compile(&section.tokens) {
                Ok(p) => (SectionKind::DateTime, SectionProgram::DateTime(p)),
                Err(e) => return Err(diagnostic(index, None, e)),
            }
        } else if fraction {
            match number::FractionPlan::compile_compact(&section.tokens) {
                Ok(p) => (SectionKind::Fraction, SectionProgram::Fraction(p)),
                Err(e) => return Err(diagnostic(index, None, e)),
            }
        } else {
            let kind = if section.tokens.iter().any(|t| matches!(t, Token::General)) {
                SectionKind::General
            } else if !section.tokens.iter().any(|t| matches!(t, Token::Digit(_))) {
                SectionKind::Literal
            } else if section
                .tokens
                .iter()
                .any(|t| matches!(t, Token::Exponent { .. }))
            {
                SectionKind::Scientific
            } else {
                SectionKind::Decimal
            };
            let decimal_source = kind == SectionKind::Decimal
                && format.sections.len() <= 2
                && format.sections.iter().all(|s| s.condition.is_none());
            // A standalone text format preserves numeric content. Its affixes
            // belong to text substitution, while numbers use General display.
            let numeric_tokens = if format.sections.len() == 1 && is_text_section(&section.tokens) {
                &[Token::General]
            } else {
                section.tokens.as_slice()
            };
            match NumberPlan::compile_with_source(numeric_tokens, decimal_source) {
                Ok(p) => (kind, SectionProgram::Number(p)),
                Err(e) => return Err(diagnostic(index, None, e)),
            }
        };
        sections.push(SectionPlan {
            kind,
            program,
            condition: section.condition,
            color: section.color.clone(),
            text_adapter: section.tokens.clone(),
        });
    }
    // Java DecimalFormat uses the positive digit layout for both subpatterns;
    // the negative subpattern contributes affixes only. CellFormat routes differ.
    if sections.len() == 2
        && sections
            .iter()
            .all(|s| s.kind == SectionKind::Decimal && s.condition.is_none())
        && let (SectionProgram::Number(positive), SectionProgram::Number(negative)) =
            // SAFETY: The short-circuit condition checks sections.len() == 2 before accessing either section.
            (&sections[0].program, &sections[1].program)
        && let Some(layout) = positive.with_negative_affixes(negative)
    {
        // SAFETY: The short-circuit condition checks sections.len() == 2 before accessing either section.
        sections[1].program = SectionProgram::Number(layout);
    }
    Ok(FormatPlan {
        sections,
        policy,
        fingerprint: program_fingerprint(format, policy),
    })
}
