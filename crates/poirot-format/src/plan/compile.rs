//! Pure validation and compilation of owned section layouts.
use super::fingerprint::program_fingerprint;
use super::{
    Format, FormatCompileError, FormatCompilePolicy, FormatError, FormatPlan, FormatSourcePolicy,
    NumberPlan, SectionKind, SectionPlan, SectionProgram, Token, ValidationPolicy, date, error,
    number, parse_format,
};

fn diagnostic(section: usize, token: Option<usize>, source: FormatError) -> FormatCompileError {
    FormatCompileError {
        section: Some(section),
        token,
        source,
    }
}

/// Parse canonical format syntax and compile it once. No render context is consulted.
pub fn compile_format(
    input: &str,
    policy: FormatCompilePolicy,
) -> Result<FormatPlan, FormatCompileError> {
    compile_format_with_source(input, policy, FormatSourcePolicy::default())
}
/// Parse with an explicit supported formatter source.
pub fn compile_format_with_source(
    input: &str,
    policy: FormatCompilePolicy,
    source: FormatSourcePolicy,
) -> Result<FormatPlan, FormatCompileError> {
    compile_program_with_source(&parse_format(input)?, policy, source)
}
/// Compile an existing parsed token program, including publicly constructed values.
pub fn compile_program(
    format: &Format,
    policy: FormatCompilePolicy,
) -> Result<FormatPlan, FormatCompileError> {
    compile_program_with_source(format, policy, FormatSourcePolicy::default())
}
/// Compile source identity into the immutable plan and its numeric layouts.
pub fn compile_program_with_source(
    format: &Format,
    policy: FormatCompilePolicy,
    source: FormatSourcePolicy,
) -> Result<FormatPlan, FormatCompileError> {
    if format.sections.is_empty() {
        return Err(error("format has no sections").into());
    }
    if format.sections.len() > 4 {
        return Err(error("too many sections").into());
    }
    let mut sections = Vec::with_capacity(format.sections.len());
    for (index, section) in format.sections.iter().enumerate() {
        if section.tokens.len() > policy.max_tokens_per_section.min(32768) {
            return Err(diagnostic(index, None, error("token limit exceeded")));
        }
        for (i, token) in section.tokens.iter().enumerate() {
            let invalid = match token {
                Token::Elapsed(c, w) if !matches!(c, 'h' | 'm' | 's') || !(1..=2).contains(w) => {
                    Some("unsupported elapsed token")
                }
                Token::Exponent { digits, .. } if !(1..=15).contains(digits) => {
                    Some("unsupported exponent width")
                }
                Token::Digit(c) if !matches!(c, '0' | '#' | '?') => {
                    Some("unsupported digit placeholder")
                }
                Token::Literal(s)
                    if s.chars().count() > policy.max_literal_characters.min(32767) =>
                {
                    Some("literal exceeds cell limit")
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
                Ok(p) => (
                    SectionKind::LegacyDateTime,
                    SectionProgram::LegacyDateTime(p),
                ),
                Err(e) if policy.validation == ValidationPolicy::SelectedSection => (
                    SectionKind::DeferredInvalidNumber,
                    SectionProgram::DeferredInvalidNumber(e),
                ),
                Err(e) => return Err(diagnostic(index, None, e)),
            }
        } else if fraction {
            match number::FractionPlan::compile_with_source(&section.tokens, source) {
                Ok(p) => (
                    SectionKind::LegacyFraction,
                    SectionProgram::LegacyFraction(p),
                ),
                Err(e) if policy.validation == ValidationPolicy::SelectedSection => (
                    SectionKind::DeferredInvalidNumber,
                    SectionProgram::DeferredInvalidNumber(e),
                ),
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
            let decimal_source = if kind == SectionKind::Decimal
                && format.sections.len() <= 2
                && format.sections.iter().all(|s| s.condition.is_none())
            {
                source
            } else {
                FormatSourcePolicy::LegacyExcelStyle
            };
            match NumberPlan::compile_with_source(
                &section.tokens,
                policy.validation == ValidationPolicy::AllSections,
                decimal_source,
            ) {
                Ok(p) => (kind, SectionProgram::Number(p)),
                Err(e) if policy.validation == ValidationPolicy::SelectedSection => (
                    SectionKind::DeferredInvalidNumber,
                    SectionProgram::DeferredInvalidNumber(e),
                ),
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
    if source == FormatSourcePolicy::Poi7321209DataFormatter
        && sections.len() == 2
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
        source,
        fingerprint: program_fingerprint(format, policy, source),
    })
}
