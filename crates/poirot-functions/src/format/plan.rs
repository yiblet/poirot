//! Compile number formats once, then render values with a date system and locale.
use super::{
    CellValue, Condition, DateSystem, Format, FormatCompileError, FormatError, FormatOptions,
    FormattedValue, NumberPlan, RenderedText, Token, date, error, fidelity_error, number,
    parse_format, render_text, resource_error,
};

mod compile;
mod execute;
mod fingerprint;

/// Recognize text substitution without interpreting mixed numeric or date tokens.
fn is_text_section(tokens: &[Token]) -> bool {
    tokens.iter().any(|token| matches!(token, Token::Text))
        && tokens.iter().all(|token| {
            matches!(
                token,
                Token::Text | Token::Literal(_) | Token::Padding(_) | Token::Fill(_)
            )
        })
}

pub use compile::{compile_format, compile_program};

pub use execute::render_value;

#[cfg(test)]
mod tests;

/// Resource bounds applied while compiling format tokens and expanding display text.
/// The compiled plan retains these limits so repeated renders honor the same output ceiling.
///
/// ```
/// use poirot_functions::format::{compile_format, render_value, FormatCompilePolicy, FormatRenderContext};
/// use poirot_functions::scalar::CellValue;
/// let policy = FormatCompilePolicy::default();
/// let plan = compile_format("0.0%", policy)?;
/// assert_eq!(render_value(&CellValue::Number(0.125), &plan,
///     FormatRenderContext::default())?.text, "12.5%");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCompilePolicy {
    /// Maximum tokens in one format section.
    pub max_tokens_per_section: usize,
    /// Maximum Unicode scalar values in a single literal token.
    pub max_literal_characters: usize,
    /// Maximum Unicode scalar values in rendered output.
    pub max_output_characters: usize,
}
impl Default for FormatCompilePolicy {
    fn default() -> Self {
        Self {
            max_tokens_per_section: 32768,
            max_literal_characters: 32767,
            max_output_characters: 32767,
        }
    }
}

/// Locale selection for displayed cell text.
///
/// English names and invariant numeric punctuation are supported.
/// System selectors use US regional formats, not the host locale:
/// `dddd, mmmm d, yyyy` and `h:mm:ss AM/PM`. Other LCIDs fail explicitly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormatLocale {
    /// Supported English names and invariant numeric punctuation.
    #[default]
    English,
    /// Preserve a requested locale rather than silently substituting English.
    UnsupportedLcid(u32),
}
impl FormatLocale {
    /// Select a display locale from a Windows locale ID.
    ///
    /// Only US English (`0x0409`) is supported. `F800` and `F400` select system
    /// formats rather than locales and must not be passed here.
    pub const fn from_lcid(lcid: u32) -> Self {
        if lcid == 0x0409 {
            Self::English
        } else {
            Self::UnsupportedLcid(lcid)
        }
    }
}
/// Value-independent inputs supplied for one render of a compiled format.
/// Date epoch, display locale, and fill width remain explicit rather than coming from process settings.
///
/// ```
/// use poirot_functions::format::{compile_format, render_value, FormatCompilePolicy, FormatRenderContext};
/// use poirot_functions::scalar::{CellValue, DateSystem};
/// let context = FormatRenderContext { date_system: DateSystem::Excel1904, ..Default::default() };
/// let plan = compile_format("yyyy-mm-dd", FormatCompilePolicy::default())?;
/// assert_eq!(render_value(&CellValue::Number(0.), &plan, context)?.text, "1904-01-01");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct FormatRenderContext {
    /// Workbook date epoch used by date/time sections.
    pub date_system: DateSystem,
    /// Unicode character width, not typography or pixel measurement.
    pub width: Option<usize>,
    /// Explicit display locale; never inferred from the host.
    pub locale: FormatLocale,
}
impl From<FormatOptions> for FormatRenderContext {
    fn from(o: FormatOptions) -> Self {
        Self {
            date_system: o.date_system,
            width: o.width,
            locale: FormatLocale::English,
        }
    }
}
/// Compiled section rendering modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// Fixed-point numeric layout.
    Decimal,
    /// Scientific numeric layout with exponent formatting.
    Scientific,
    /// General-value formatting with literal fragments.
    General,
    /// Literal or text-placeholder section.
    Literal,
    /// Fraction layout, such as `# ?/?`.
    Fraction,
    /// Compiled calendar or elapsed-time layout.
    DateTime,
}
/// The numeric, fraction, or date renderer selected for a format section.
#[derive(Debug, Clone, PartialEq)]
enum SectionProgram {
    Number(NumberPlan),
    Fraction(number::FractionPlan),
    DateTime(date::DatePlan),
}
/// One compiled format branch, combining its selection condition and color with
/// a numeric, fraction, or date program and the tokens used for text substitution.
/// It is reused after section selection without reparsing the format source.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionPlan {
    kind: SectionKind,
    program: SectionProgram,
    condition: Option<Condition>,
    color: Option<String>,
    text_adapter: Vec<Token>,
}
impl SectionPlan {
    /// Inspect the section's compiled rendering mode.
    pub fn kind(&self) -> SectionKind {
        self.kind
    }
    /// Return the numeric predicate controlling section selection.
    pub fn condition(&self) -> Option<Condition> {
        self.condition
    }
    /// Borrow this section's optional display color.
    pub fn color(&self) -> Option<&str> {
        self.color.as_deref()
    }
    /// Borrow the numeric program when this section uses one.
    pub fn number_plan(&self) -> Option<&NumberPlan> {
        match &self.program {
            SectionProgram::Number(p) => Some(p),
            _ => None,
        }
    }

    /// Whether text values pass through the section's text-substitution adapter.
    pub fn uses_text_adapter(&self) -> bool {
        true
    }
}
/// A compiled number format that can be reused for multiple values.
///
/// ```
/// use poirot_functions::format::{compile_format, render_value, FormatCompilePolicy, FormatRenderContext};
/// use poirot_functions::scalar::CellValue;
/// let plan = compile_format("0.00", FormatCompilePolicy::default())?;
/// for (value, expected) in [(12.5, "12.50"), (3., "3.00")] {
///     let displayed = render_value(&CellValue::Number(value), &plan, FormatRenderContext::default())?;
///     assert_eq!(displayed.text, expected);
/// }
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct FormatPlan {
    sections: Vec<SectionPlan>,
    policy: FormatCompilePolicy,
    fingerprint: u64,
}
impl FormatPlan {
    /// Borrow immutable compiled sections in source order.
    pub fn sections(&self) -> &[SectionPlan] {
        &self.sections
    }
    /// Return the resource limits used to compile this format.
    pub fn policy(&self) -> FormatCompilePolicy {
        self.policy
    }
    /// Deterministic, noncryptographic program/policy fingerprint. It is not a unique
    /// identity or a substitute for equality when authorizing cache reuse.
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}
