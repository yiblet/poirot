//! Immutable number-format contracts shared by numeric and fraction execution.

use super::{FormatError, RenderedText, Token};

mod compile;
mod fraction;
mod render;

/// Immutable placeholder layout compiled independently of a value or render context.
#[derive(Debug, Clone, PartialEq)]
pub struct NumberPlan {
    program: NumberProgram,
}
/// Compiled numeric layout selected for a reusable number plan.
#[derive(Debug, Clone, PartialEq)]
enum NumberProgram {
    General(Vec<GeneralPart>),
    Literal(RenderedText),
    Numeric(NumericPlan),
}
/// Literal or General-value fragment in a compiled General section.
#[derive(Debug, Clone, PartialEq)]
enum GeneralPart {
    Value,
    Literal(RenderedText),
}
/// Resolved placeholders, scaling, affixes, and exponent layout for numeric rendering.
#[derive(Debug, Clone, PartialEq)]
struct NumericPlan {
    integer: Vec<char>,
    decimal: Vec<char>,
    has_decimal: bool,
    min_integer: usize,
    minimum_visible_digit: bool,
    question_count: usize,
    grouping: bool,
    scaling: i32,
    percentages: i32,
    exponent: Option<(bool, usize)>,
    prefix: RenderedText,
    suffix: RenderedText,
}
/// Borrowed compilation evidence; no placeholder layout is rediscovered by rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberLayout<'a> {
    /// Integral digit placeholders in display order.
    pub integer_placeholders: &'a [char],
    /// Fractional digit placeholders in display order.
    pub decimal_placeholders: &'a [char],
    /// Whether integer digits use thousands grouping.
    pub grouping: bool,
    /// Trailing comma count used to scale by powers of one thousand.
    pub scaling_commas: i32,
    /// Percent marker count used to scale by powers of one hundred.
    pub percent_signs: i32,
    /// Optional scientific exponent layout.
    pub scientific_exponent: Option<(bool, usize)>,
    /// Affix emitted before the formatted number.
    pub prefix: &'a str,
    /// Affix emitted after the formatted number.
    pub suffix: &'a str,
}
impl NumberPlan {
    /// Inspect the compiled numeric layout when this program has one.
    pub fn layout(&self) -> Option<NumberLayout<'_>> {
        match &self.program {
            NumberProgram::Numeric(p) => Some(NumberLayout {
                integer_placeholders: &p.integer,
                decimal_placeholders: &p.decimal,
                grouping: p.grouping,
                scaling_commas: p.scaling,
                percent_signs: p.percentages,
                scientific_exponent: p.exponent,
                prefix: &p.prefix.text,
                suffix: &p.suffix.text,
            }),
            _ => None,
        }
    }
    /// Compile tokens under the selected source-compatibility contract.
    pub(super) fn compile_with_source(
        tokens: &[Token],
        decimal_format: bool,
    ) -> Result<Self, FormatError> {
        let mut plan = Self::compile_numeric(tokens)?;
        if decimal_format && let NumberProgram::Numeric(p) = &mut plan.program {
            // DecimalFormat normalization excludes spacing placeholders.
            if !p.integer.contains(&'?') && !p.decimal.contains(&'?') {
                p.minimum_visible_digit = true;
                if p.min_integer == 0 && !p.decimal.contains(&'0') && p.has_decimal {
                    if p.integer.is_empty() && !p.decimal.is_empty() {
                        // SAFETY: The immediately enclosing condition requires p.decimal to be nonempty.
                        p.decimal[0] = '0';
                    } else {
                        p.min_integer = 1;
                    }
                }
            }
        }
        Ok(plan)
    }
    /// Apply section-specific negative-number affixes to the compiled layout.
    pub(super) fn with_negative_affixes(&self, negative: &Self) -> Option<Self> {
        let (NumberProgram::Numeric(positive), NumberProgram::Numeric(negative)) =
            (&self.program, &negative.program)
        else {
            return None;
        };
        let mut layout = positive.clone();
        layout.prefix = negative.prefix.clone();
        layout.suffix = negative.suffix.clone();
        Some(Self {
            program: NumberProgram::Numeric(layout),
        })
    }
    /// Render without character-width information; actual fill directives require FormatPlan context.
    pub fn render(&self, n: f64) -> Result<String, FormatError> {
        self.render_output(n)?.finish(None, 32767)
    }
    /// Render a value while retaining literal-span metadata.
    pub(super) fn render_output(&self, n: f64) -> Result<RenderedText, FormatError> {
        self.render_numeric_output(n)
    }
}

/// Immutable bounded fraction layout; raw tokens are interpreted only at compilation.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FractionPlan {
    compact: bool,
    numerator_width: usize,
    denominator_width: usize,
    denominator: FractionDenominator,
    mixed: bool,
    required_whole: bool,
    prefix: RenderedText,
    separator: RenderedText,
    suffix: RenderedText,
}
/// Fixed or placeholder-bounded denominator used by fraction approximation.
#[derive(Debug, Clone, PartialEq)]
enum FractionDenominator {
    Fixed(u32),
    Variable(u64),
}

impl FractionPlan {
    /// Compile a fraction layout using the default source contract.
    pub(super) fn compile(tokens: &[Token]) -> Result<Self, FormatError> {
        Self::compile_fraction(tokens)
    }
    /// Validate fraction tokens under an explicit compatibility contract.
    pub(super) fn compile_with_source(
        tokens: &[Token],
        source: crate::FormatSourcePolicy,
    ) -> Result<Self, FormatError> {
        let mut plan = Self::compile(tokens)?;
        plan.compact = source == crate::FormatSourcePolicy::Poi7321209DataFormatter;
        Ok(plan)
    }
    /// Render a fraction value with literal-span metadata.
    pub(super) fn render_output(&self, n: f64) -> Result<RenderedText, FormatError> {
        self.render_fraction_output(n)
    }
}
