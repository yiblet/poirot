//! Compile number formats once, then render values with a date system and locale.
use super::*;

mod compile;
mod execute;
mod fingerprint;

pub use compile::{
    compile_format, compile_format_with_source, compile_program, compile_program_with_source,
};

/// Selects the formatter behavior used during compilation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormatSourcePolicy {
    /// Use the pinned POI DataFormatter source contract.
    #[default]
    Poi7321209DataFormatter,
}
pub use execute::render_value;

#[cfg(test)]
mod tests;

/// Limits for format compilation and rendered output.
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
/// Date system, locale, and optional width used to render a cell value.
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
/// An error compiling a number format, with its section and token when known.
///
/// Errors from parsing format text also retain the original UTF-8 byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatCompileError {
    /// Zero-based failing section index, if known.
    pub section: Option<usize>,
    /// Zero-based failing token index within the section, if known.
    pub token: Option<usize>,
    /// Underlying syntax, fidelity, or resource diagnostic.
    pub source: FormatError,
}
impl fmt::Display for FormatCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(s) = self.section {
            write!(f, "section {s}")?;
            if let Some(t) = self.token {
                write!(f, ", token {t}")?;
            }
            write!(f, ": ")?;
        }
        self.source.fmt(f)
    }
}
impl std::error::Error for FormatCompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}
impl From<FormatError> for FormatCompileError {
    fn from(source: FormatError) -> Self {
        Self {
            section: None,
            token: None,
            source,
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
    /// Literal-only section.
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
/// Immutable section with text-substitution tokens.
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
#[derive(Debug, Clone, PartialEq)]
pub struct FormatPlan {
    sections: Vec<SectionPlan>,
    policy: FormatCompilePolicy,
    fingerprint: u64,
    source: FormatSourcePolicy,
}
impl FormatPlan {
    /// Return the compatibility policy used to compile this format.
    pub fn source_policy(&self) -> FormatSourcePolicy {
        self.source
    }
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
