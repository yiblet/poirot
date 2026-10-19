//! Typed execution and conservative preflight reservation share expansion assumptions.
use super::{
    CellValue, FormatError, FormatPlan, FormatRenderContext, FormattedValue, RenderedText,
    SectionPlan, SectionProgram, Token, error, fidelity_error, render_text, resource_error,
};

impl FormatPlan {
    /// Conservative UTF-8 preflight reservation before rendering allocates output.
    /// Includes intermediate token expansion, text substitution, and Unicode fill.
    /// This is an upper bound, not the eventual output length; callers can charge
    /// it to a budget before calling `render_value` and release unused reservation.
    pub fn render_byte_bound(
        &self,
        value: &CellValue,
        context: FormatRenderContext,
    ) -> Result<usize, FormatError> {
        if context.width.is_some_and(|w| w > 32767) {
            return Err(resource_error("width exceeds resource limit"));
        }
        let text_bytes = value
            .text_ref()
            .map(|text| text.utf8_len().map_err(|_| fidelity_error()))
            .transpose()?
            .unwrap_or(0);
        let mut bound = text_bytes.max(512);
        for section in &self.sections {
            // Finite f64 fixed notation needs at most 309 integer digits;
            // sign, decimal point, grouping commas and a <=15-digit exponent
            // fit within 512 bytes. Each placeholder contributes at most two
            // bytes including grouping. Fraction layouts have one byte per
            // placeholder plus <=16 whole digits. Date/elapsed fields need at
            // most 20 bytes (elapsed seconds <=12, names <=9, fractions <=7).
            // 32 bytes per ordinary token covers those expansions, including
            // UTF-8 literals produced by a single character token.
            bound = bound
                .checked_add(512)
                .ok_or_else(|| resource_error("render byte bound overflow"))?;
            for token in &section.text_adapter {
                let bytes = match token {
                    Token::Literal(s) => s.len(),
                    Token::Text | Token::General => text_bytes.max(512),
                    _ => 32,
                };
                bound = bound
                    .checked_add(bytes)
                    .ok_or_else(|| resource_error("render byte bound overflow"))?;
            }
        }
        bound
            .checked_add(context.width.unwrap_or(0) * 4)
            .ok_or_else(|| resource_error("render byte bound overflow"))
    }
}

fn select(plan: &FormatPlan, n: f64) -> Result<(&SectionPlan, bool), FormatError> {
    // SAFETY: min(3) bounds the slice end by plan.sections.len().
    let numeric = &plan.sections[..plan.sections.len().min(3)];
    if numeric.iter().any(|s| s.condition.is_some()) {
        for s in numeric {
            if s.condition.is_none_or(|c| c.matches(n)) {
                return Ok((s, false));
            }
        }
        return Err(error("no conditional format section matches"));
    }
    let i = if n < 0. && numeric.len() > 1 {
        1
    } else if n == 0. && numeric.len() > 2 {
        2
    } else {
        0
    };
    Ok((
        numeric
            .get(i)
            .ok_or_else(|| error("format has no numeric sections"))?,
        i == 1,
    ))
}
/// Execute an immutable plan using precomputed decimal/scientific/fraction layouts.
/// General, date, and text sections render from compiled layouts.
pub fn render_value(
    value: &CellValue,
    plan: &FormatPlan,
    context: FormatRenderContext,
) -> Result<FormattedValue, FormatError> {
    if context.width.is_some_and(|w| w > 32767) {
        return Err(resource_error("width exceeds resource limit"));
    }
    let (section, absolute) = match value {
        CellValue::Number(n) => {
            if !n.is_finite() {
                return Err(error("non-finite number"));
            }
            let (s, a) = select(plan, *n)?;
            (Some(s), a)
        }
        CellValue::Text(_) | CellValue::TextUtf16(_) => (plan.sections.get(3), false),
        _ => (plan.sections.first(), false),
    };
    let text = match value {
        CellValue::Blank => RenderedText::default(),
        CellValue::Error(e) => RenderedText::plain(e.to_string()),
        CellValue::Boolean(b) => RenderedText::plain(if *b { "TRUE" } else { "FALSE" }),
        CellValue::Text(_) | CellValue::TextUtf16(_) => {
            let source = value.text_ref().ok_or_else(fidelity_error)?;
            // Measurement rejects lone units without allocating a UTF-8 projection.
            source.utf8_len().map_err(|_| fidelity_error())?;
            let characters = std::char::decode_utf16(source.units())
                .try_fold(0usize, |count, scalar| scalar.map(|_| count + 1))
                .map_err(|_| fidelity_error())?;
            let mut output = if section.is_none() { characters } else { 0 };
            if let Some(section) = section {
                for token in &section.text_adapter {
                    let extra = match token {
                        Token::Text | Token::General => characters,
                        Token::Literal(s) => s.chars().count(),
                        Token::Padding(_) | Token::Percent => 1,
                        _ => 0,
                    };
                    output = output
                        .checked_add(extra)
                        .ok_or_else(|| resource_error("formatted text count overflow"))?;
                }
            }
            if output > plan.policy.max_output_characters.min(32767) {
                return Err(resource_error("formatted text exceeds cell limit"));
            }
            let source = source.to_utf8().map_err(|_| fidelity_error())?;
            match section {
                None => RenderedText::plain(source.into_owned()),
                Some(p) => render_text(&p.text_adapter, &source),
            }
        }
        CellValue::Number(n) => {
            let n = if absolute { n.abs() } else { *n };
            match &section
                .ok_or_else(|| error("format has no numeric sections"))?
                .program
            {
                SectionProgram::Number(p) => p.render_output(n)?,
                SectionProgram::Fraction(p) => p.render_output(n)?,
                SectionProgram::DateTime(t) => t.render(n, context.date_system, context.locale)?,
            }
        }
    };
    let text = text.finish(context.width, plan.policy.max_output_characters)?;
    Ok(FormattedValue {
        text,
        color: section.and_then(|s| s.color.clone()),
    })
}
