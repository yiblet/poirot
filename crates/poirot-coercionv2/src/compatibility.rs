//! Explicit compatibility profiles for existing argument consumers.
use poirot_functions_core::{CellError, CellValue, Scalar, TextRef};
use poirot_numeric::number_text::render;
pub fn coercev2_criteria_number_text(s: &str) -> Option<f64> {
    poirot_parsing::number::parse_criteria_number(s)
}

pub fn coercev2_financial_decimal_text(text: &str) -> Option<f64> {
    poirot_parsing::number::parse_financial_decimal(text)
}
pub fn coercev2_statistics_decimal_text(text: &str) -> Option<f64> {
    poirot_parsing::number::parse_statistics_decimal(text)
}

/// Borrowed scalar view; omission remains distinct from a blank cell.
#[derive(Clone, Copy, Debug)]
pub enum ScalarRef<'a> {
    /// Existing number, including its original IEEE bits.
    Number(f64),
    /// Existing logical value.
    Boolean(bool),
    /// Text retained in its original representation.
    Text(TextRef<'a>),
    /// Empty cell.
    Blank,
    /// Omitted argument slot.
    Missing,
    /// Worksheet failure.
    Error(CellError),
}
impl<'a> From<&'a Scalar> for ScalarRef<'a> {
    fn from(value: &'a Scalar) -> Self {
        match value {
            Scalar::Number(n) => Self::Number(*n),
            Scalar::Boolean(b) => Self::Boolean(*b),
            Scalar::Text(t) => Self::Text(TextRef::Utf8(t)),
            Scalar::Blank => Self::Blank,
            Scalar::Missing => Self::Missing,
            Scalar::Error(e) => Self::Error(*e),
        }
    }
}
impl<'a> From<&'a CellValue> for ScalarRef<'a> {
    fn from(value: &'a CellValue) -> Self {
        match value {
            CellValue::Number(n) => Self::Number(*n),
            CellValue::Boolean(b) => Self::Boolean(*b),
            CellValue::Text(t) => Self::Text(TextRef::Utf8(t)),
            CellValue::TextUtf16(t) => Self::Text(TextRef::Utf16(t.units())),
            CellValue::Blank => Self::Blank,
            CellValue::Error(e) => Self::Error(*e),
        }
    }
}
/// Numeric admission chosen by the consuming function or operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberPolicy {
    /// Preserve existing and parsed numbers, including nonfinite values.
    Preserve,
    /// Reject any nonfinite result with NUM.
    Finite,
}
/// Inputs requiring host effects before numeric conversion can finish.
#[derive(Clone, Copy, Debug)]
pub enum DeferredNumber<'a> {
    /// Text admission, parsing grammar, and optional context-dependent fallback.
    Text(TextRef<'a>),
    /// Consumer-specific omitted-argument capability.
    Missing,
}
/// Central scalar dispatch with a metered host continuation for text/omission.
/// The continuation is never called for numeric, logical, blank, or error inputs.
pub fn coercev2_number<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    policy: NumberPolicy,
    mut deferred: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<f64, E> {
    let number = match value {
        ScalarRef::Number(n) => n,
        ScalarRef::Boolean(b) => f64::from(u8::from(b)),
        ScalarRef::Blank => 0.0,
        ScalarRef::Error(e) => return Err(e.into()),
        ScalarRef::Text(t) => deferred(DeferredNumber::Text(t))?,
        ScalarRef::Missing => deferred(DeferredNumber::Missing)?,
    };
    if policy == NumberPolicy::Finite && !number.is_finite() {
        Err(CellError::Num.into())
    } else {
        Ok(number)
    }
}
/// Legacy Unicode-trimmed, finite Rust numeric grammar.
pub fn coercev2_arithmetic_text(text: &str) -> Result<f64, CellError> {
    poirot_parsing::number::parse_rust_float(text.trim())
        .filter(|n| n.is_finite())
        .ok_or(CellError::Value)
}
/// Logical input restrictions; missing slots are returned separately to the host.
#[derive(Clone, Copy, Debug)]
pub struct BooleanPolicy {
    /// Reject NaN with VALUE while retaining infinity's truth value.
    pub reject_nan: bool,
    /// Whether an actual blank cell is accepted as false.
    pub blank: bool,
}
/// Convert admitted boolean input without projecting malformed UTF16 to Unicode.
/// `None` represents an omitted slot; it is never substituted for a blank.
pub fn coercev2_boolean(
    value: ScalarRef<'_>,
    policy: BooleanPolicy,
) -> Result<Option<bool>, CellError> {
    match value {
        ScalarRef::Missing => Ok(None),
        ScalarRef::Blank if policy.blank => Ok(Some(false)),
        ScalarRef::Blank => Err(CellError::Value),
        ScalarRef::Boolean(b) => Ok(Some(b)),
        ScalarRef::Number(n) if policy.reject_nan && n.is_nan() => Err(CellError::Value),
        ScalarRef::Number(n) => Ok(Some(n != 0.0)),
        ScalarRef::Error(e) => Err(e),
        ScalarRef::Text(text) => {
            let equal = |expected: &[u8]| {
                let length = match text {
                    TextRef::Utf8(t) => t.len(),
                    TextRef::Utf16(t) => t.len(),
                };
                length == expected.len()
                    && text
                        .units()
                        .zip(expected)
                        .all(|(unit, byte)| unit <= 127 && (unit as u8).eq_ignore_ascii_case(byte))
            };
            if equal(b"TRUE") {
                Ok(Some(true))
            } else if equal(b"FALSE") {
                Ok(Some(false))
            } else {
                Err(CellError::Value)
            }
        }
    }
}
/// Logical function condition policy, including source NaN rejection.
pub fn coercev2_logical_test(value: &CellValue) -> Result<bool, CellError> {
    coercev2_boolean(
        value.into(),
        BooleanPolicy {
            reject_nan: true,
            blank: true,
        },
    )?
    .ok_or(CellError::Value)
}
/// Shared source number spelling; callers admit their required output bound first.
pub fn coercev2_number_to_text(number: f64) -> String {
    render(number)
}
/// Minimum rank admission for the existing ceiling-based kth selection consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KthRankPolicy {
    /// Apache POI rejects ranks below one before rounding upward.
    PinnedPoi,
    /// Documented LARGE accepts every positive rank before rounding upward.
    DocumentedLarge,
}

/// Admit a finite kth rank before collection acquisition, retaining the source bound.
/// This policy does not apply to percentile interpolation or other integer arguments.
pub fn coercev2_statistics_kth_rank(number: f64, policy: KthRankPolicy) -> Result<f64, CellError> {
    let invalid_minimum = match policy {
        KthRankPolicy::PinnedPoi => number < 1.0,
        KthRankPolicy::DocumentedLarge => number <= 0.0,
    };
    let rank = number.ceil();
    if !number.is_finite() || invalid_minimum || rank > i32::MAX as f64 {
        return Err(CellError::Num);
    }
    Ok(rank)
}

/// Source floor-to-int conversion, retaining Java's NaN-to-zero cast behavior.
pub fn coercev2_floor_i32(number: f64) -> Result<i32, CellError> {
    let number = number.floor();
    if number < i32::MIN as f64 || number > i32::MAX as f64 {
        Err(CellError::Num)
    } else {
        Ok(number as i32)
    }
}
/// Numeric database channel: text, blank, and errors are deliberately not admitted.
pub fn coercev2_database_number(value: &CellValue) -> Option<f64> {
    match value {
        CellValue::Number(n) => Some(*n),
        CellValue::Boolean(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}
/// Formula text after host admission. Unicode projection failures remain distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFailure {
    /// Worksheet error propagated by the consumer.
    Worksheet(CellError),
    /// Text cannot be projected without replacing isolated UTF16 surrogates.
    UnsupportedText,
    /// An omitted argument needs a consumer-specific rule.
    Missing,
}
/// Shared scalar text rule. Number spelling is supplied by the selected source profile.
pub fn coercev2_text(
    value: ScalarRef<'_>,
    number: impl FnOnce(f64) -> String,
) -> Result<String, TextFailure> {
    match value {
        ScalarRef::Blank => Ok(String::new()),
        ScalarRef::Boolean(b) => Ok(if b { "TRUE" } else { "FALSE" }.into()),
        ScalarRef::Number(n) => Ok(number(n)),
        ScalarRef::Text(TextRef::Utf8(t)) => Ok(t.into()),
        ScalarRef::Text(TextRef::Utf16(t)) => {
            String::from_utf16(t).map_err(|_| TextFailure::UnsupportedText)
        }
        ScalarRef::Error(e) => Err(TextFailure::Worksheet(e)),
        ScalarRef::Missing => Err(TextFailure::Missing),
    }
}

/// Text-producing requests that require consumer-specific host admission.
pub enum DeferredText<'a> {
    /// Number rendering after the host reserves its output bound.
    Number(f64),
    /// Unicode projection after text/work admission.
    Text(TextRef<'a>),
    /// Omitted slot whose default or refusal is consumer-specific.
    Missing,
}
/// Central text dispatch with ordered, fallible host effects.
pub fn coercev2_text_with<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    mut deferred: impl FnMut(DeferredText<'a>) -> Result<String, E>,
) -> Result<String, E> {
    match value {
        ScalarRef::Blank => Ok(String::new()),
        ScalarRef::Boolean(b) => Ok(if b { "TRUE" } else { "FALSE" }.into()),
        ScalarRef::Error(e) => Err(e.into()),
        ScalarRef::Number(n) => deferred(DeferredText::Number(n)),
        ScalarRef::Text(t) => deferred(DeferredText::Text(t)),
        ScalarRef::Missing => deferred(DeferredText::Missing),
    }
}
/// NumberEval's public string view canonicalizes nonfinite inputs for this profile.
pub fn coercev2_statistics_number_text(number: f64, pinned: bool) -> String {
    if pinned && !number.is_finite() {
        if number.is_nan() {
            "2.6965397022935E+308".into()
        } else {
            "1.7976931348623E+308".into()
        }
    } else {
        coercev2_number_to_text(number)
    }
}
/// Finite signed-32 conversion with optional flooring before bounds validation.
pub fn coercev2_integer(number: f64, floor: bool) -> Result<i32, CellError> {
    let number = if floor { number.floor() } else { number };
    if !number.is_finite() || number < i32::MIN as f64 || number > i32::MAX as f64 {
        Err(CellError::Num)
    } else {
        Ok(number as i32)
    }
}
/// INDEX uses the signed source bound, then rejects negative ordinals with VALUE.
pub fn coercev2_index_ordinal(number: f64) -> Result<usize, CellError> {
    let number = coercev2_integer(number, true)?;
    if number < 0 {
        Err(CellError::Value)
    } else {
        Ok(number as usize)
    }
}
/// Truncate a date component only within the original finite signed-32 range.
pub fn coercev2_date_integer(number: f64) -> Result<i64, CellError> {
    if number.is_finite() && number >= f64::from(i32::MIN) && number <= f64::from(i32::MAX) {
        Ok(number.trunc() as i64)
    } else {
        Err(CellError::Num)
    }
}
/// N converts logical values to numbers and nonnumeric, nonerror values to zero.
pub fn coercev2_information_n(value: CellValue) -> CellValue {
    match value {
        CellValue::Number(_) | CellValue::Error(_) => value,
        CellValue::Boolean(b) => CellValue::Number(f64::from(u8::from(b))),
        _ => CellValue::Number(0.0),
    }
}
/// T retains text/errors and converts the remaining scalar kinds to empty text.
pub fn coercev2_information_t(value: CellValue) -> CellValue {
    match value {
        CellValue::Text(_) | CellValue::TextUtf16(_) | CellValue::Error(_) => value,
        _ => CellValue::Text(String::new()),
    }
}
/// Equality's numeric channel admits a blank only against an existing number.
pub fn coercev2_equality_numeric_pair(left: &CellValue, right: &CellValue) -> Option<(f64, f64)> {
    match (left, right) {
        (CellValue::Number(a), CellValue::Number(b)) => Some((*a, *b)),
        (CellValue::Blank, CellValue::Number(b)) => Some((0., *b)),
        (CellValue::Number(a), CellValue::Blank) => Some((*a, 0.)),
        _ => None,
    }
}
/// Blank comparison operands borrow the representation selected by their counterpart.
pub fn coercev2_comparison_blank<'a>(
    value: &'a CellValue,
    counterpart: &CellValue,
    zero: &'a CellValue,
    empty: &'a CellValue,
    false_value: &'a CellValue,
) -> &'a CellValue {
    if matches!(value, CellValue::Blank) {
        match counterpart {
            CellValue::Text(_) | CellValue::TextUtf16(_) => empty,
            CellValue::Boolean(_) => false_value,
            _ => zero,
        }
    } else {
        value
    }
}
/// Classic lookup treats a blank key as zero without changing the stored input.
pub fn coercev2_classic_lookup_blank_key<'a>(
    value: &'a CellValue,
    zero: &'a CellValue,
) -> &'a CellValue {
    if matches!(value, CellValue::Blank) {
        zero
    } else {
        value
    }
}
/// Pinned numeric grammar distinguishes invalid numeric values from date fallback.
pub enum NumericTextDecision {
    /// The numeric grammar matched; its numeric result may still be invalid.
    Number(Result<f64, CellError>),
    /// The numeric grammar did not match; consult the date/time rule.
    DateTime,
}
/// Source-compatible recognition before a context-aware date fallback.
pub fn coercev2_numeric_text(text: &str) -> NumericTextDecision {
    use nom::Parser;
    let trimmed = text.trim_matches(|c: char| (c as u32) <= 32);
    if nom::combinator::all_consuming(poirot_parsing::number::source_float)
        .parse(trimmed)
        .is_ok()
    {
        NumericTextDecision::Number(
            poirot_parsing::java_number::parse_java_number(trimmed)
                .filter(|n| n.is_finite())
                .ok_or(CellError::Value),
        )
    } else {
        NumericTextDecision::DateTime
    }
}

/// Delimiter omission has an empty-text default; ordinary string arguments defer it.
pub fn coercev2_text_argument<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    delimiter: bool,
    deferred: impl FnMut(DeferredText<'a>) -> Result<String, E>,
) -> Result<String, E> {
    if delimiter && matches!(value, ScalarRef::Missing) {
        Ok(String::new())
    } else {
        coercev2_text_with(value, deferred)
    }
}
/// Legacy CONCAT renders omitted members like blank, after traversal preserves
/// omission separately from the stored value. Other text consumers keep their
/// own omission policy (including pinned POI's unsupported MissingArgEval).
pub fn coercev2_concat_member_text<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    deferred: impl FnMut(DeferredText<'a>) -> Result<String, E>,
) -> Result<String, E> {
    coercev2_text_with(
        if matches!(value, ScalarRef::Missing) {
            ScalarRef::Blank
        } else {
            value
        },
        deferred,
    )
}

/// Numeric text grammar requested by TEXT's value argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFormatPolicy {
    /// Preserve the pinned arithmetic grammar and UTF8 fast path.
    PinnedPoi,
    /// Defer both text representations to the host's contextual numeric grammar.
    Contextual,
}

/// TEXT retains nonnumeric strings, converts blank to zero, and spells booleans directly.
/// The continuation returns `None` for rejected text, preserving the original value;
/// operational failures escape. The host supplies the selected contextual grammar
/// and checked UTF16 projection without duplicating TEXT's value admission rules.
pub fn coercev2_text_format_value<E>(
    value: CellValue,
    policy: TextFormatPolicy,
    project: impl FnOnce(TextRef<'_>) -> Result<Option<f64>, E>,
) -> Result<(CellValue, bool), E> {
    match value {
        CellValue::Blank => Ok((CellValue::Number(0.), true)),
        CellValue::Text(ref text) if policy == TextFormatPolicy::PinnedPoi => {
            match coercev2_arithmetic_text(text) {
                Ok(n) => Ok((CellValue::Number(n), true)),
                Err(_) => Ok((value, false)),
            }
        }
        CellValue::Text(ref text) => match project(TextRef::Utf8(text))? {
            Some(n) => Ok((CellValue::Number(n), true)),
            None => Ok((value, false)),
        },
        CellValue::TextUtf16(ref text) => match project(TextRef::Utf16(text.units()))? {
            Some(n) => Ok((CellValue::Number(n), true)),
            None => Ok((value, false)),
        },
        CellValue::Boolean(b) => Ok((
            CellValue::Text(if b { "TRUE" } else { "FALSE" }.into()),
            false,
        )),
        _ => Ok((value, true)),
    }
}
/// Explicit omission projection, retaining separate provenance at the host.
#[derive(Clone, Copy, Debug)]
pub enum MissingScalarPolicy {
    /// Legacy scalar representation without changing the host omission flag.
    Blank,
    /// Numeric zero default.
    Zero,
    /// Reject omitted input.
    Reject,
}
/// Requested scalar representation of an omitted slot.
pub fn coercev2_missing_scalar(policy: MissingScalarPolicy) -> Result<CellValue, CellError> {
    match policy {
        MissingScalarPolicy::Blank => Ok(CellValue::Blank),
        MissingScalarPolicy::Zero => Ok(CellValue::Number(0.)),
        MissingScalarPolicy::Reject => Err(CellError::Value),
    }
}
/// Selected rendering profile and finite admission for numeric string arguments.
pub fn coercev2_number_text_finite(value: f64) -> Result<String, CellError> {
    if value.is_finite() {
        Ok(render(value))
    } else {
        Err(CellError::Num)
    }
}
/// Existing numeric operator operands retain their bits before outcome handling.
pub fn coercev2_arithmetic_operand<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    deferred: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<f64, E> {
    if let ScalarRef::Number(n) = value {
        Ok(n)
    } else {
        coercev2_number(value, NumberPolicy::Finite, deferred)
    }
}

/// Date numeric text rejects nonfinite parses before considering date fallback.
pub fn coercev2_date_numeric_text(text: &str) -> NumericTextDecision {
    match poirot_parsing::number::parse_rust_float(text.trim()) {
        Some(n) => NumericTextDecision::Number(if n.is_finite() {
            Ok(n)
        } else {
            Err(CellError::Value)
        }),
        None => NumericTextDecision::DateTime,
    }
}
/// Source-visible decimal text converted back to a truncated logical-function index.
pub fn coercev2_truncated_text(text: &str) -> Result<f64, CellError> {
    poirot_parsing::number::parse_rust_float(text)
        .map(f64::trunc)
        .ok_or(CellError::Num)
}
/// Number rendering with an explicit source profile, following host admission.
pub fn coercev2_render_number(value: f64) -> String {
    render(value)
}
/// Reject nonfinite input before asking the host to reserve rendering resources.
pub fn coercev2_render_finite_number<E: From<CellError>>(
    value: f64,
    admit: impl FnOnce() -> Result<(), E>,
) -> Result<String, E> {
    if !value.is_finite() {
        return Err(CellError::Num.into());
    }
    admit()?;
    Ok(render(value))
}

/// Source database field rounding and bounds.
pub fn coercev2_database_field_index(value: &CellValue, columns: usize) -> Option<usize> {
    let value = coercev2_database_number(value)?;
    if !value.is_finite() {
        return None;
    }
    let lower = value.floor();
    let rounded = lower + if value - lower >= 0.5 { 1. } else { 0. };
    let column = rounded - 1.;
    if rounded < i32::MIN as f64 || rounded > i32::MAX as f64 {
        return None;
    }
    (column >= 0. && column < columns as f64).then_some(column as usize)
}
/// Source database criteria header floor and bounds.
pub fn coercev2_database_criteria_header_index(
    value: &CellValue,
    columns: usize,
) -> Result<Option<usize>, CellError> {
    let index = coercev2_database_number(value)
        .ok_or(CellError::Value)?
        .floor();
    if !index.is_finite() || index < i32::MIN as f64 || index > i32::MAX as f64 {
        return Err(CellError::Value);
    }
    let column = index - 1.;
    Ok(if column >= 0. && column < columns as f64 {
        Some(column as usize)
    } else {
        None
    })
}

/// Java numeric syntax, including hex and suffixes, selected explicitly by its consumers.
pub fn coercev2_java_number_text(text: &str) -> Option<f64> {
    poirot_parsing::java_number::parse_java_number(text)
}
/// Optional numeric argument defaults only omitted slots, never blank cells.
pub fn coercev2_optional_number<E>(
    value: Scalar,
    default: f64,
    convert: impl FnOnce(Scalar) -> Result<f64, E>,
) -> Result<f64, E> {
    if matches!(value, Scalar::Missing) {
        Ok(default)
    } else {
        convert(value)
    }
}
/// Documented payment operands default absent optional slots and explicit
/// positional Missing to numeric zero.
/// Arity admission separately rejects absent required positions. Blank and other
/// scalar values continue through the consumer's numeric policy.
pub fn coercev2_financial_payment_number<E>(
    value: Option<Scalar>,
    convert: impl FnOnce(Scalar) -> Result<f64, E>,
) -> Result<f64, E> {
    match value {
        Some(value) => coercev2_optional_number(value, 0.0, convert),
        None => Ok(0.0),
    }
}
/// Documented IPMT/PPMT timing admits exactly end (zero) or beginning (one).
/// Numeric parsing and omission defaults are selected before this policy.
pub fn coercev2_financial_payment_timing(value: f64) -> Result<bool, CellError> {
    match value {
        0.0 => Ok(false),
        1.0 => Ok(true),
        _ => Err(CellError::Num),
    }
}
/// Documented boolean options use false for omission and preserve their other restrictions.
pub fn coercev2_boolean_default(
    value: ScalarRef<'_>,
    policy: BooleanPolicy,
) -> Result<bool, CellError> {
    Ok(coercev2_boolean(value, policy)?.unwrap_or(false))
}

/// Owned text requests let adapters transfer existing buffers without another allocation.
pub enum OwnedDeferredText {
    /// Metered number rendering.
    Number(f64),
    /// Existing text buffer, transferred unchanged.
    Text(String),
    /// Omitted slot needing a consumer-specific capability/default.
    Missing,
}
/// The owned counterpart of scalar text dispatch; it does not clone text inputs.
pub fn coercev2_owned_text<E: From<CellError>>(
    value: Scalar,
    mut deferred: impl FnMut(OwnedDeferredText) -> Result<String, E>,
) -> Result<String, E> {
    match value {
        Scalar::Blank => Ok(String::new()),
        Scalar::Boolean(b) => Ok(if b { "TRUE" } else { "FALSE" }.into()),
        Scalar::Error(e) => Err(e.into()),
        Scalar::Number(n) => deferred(OwnedDeferredText::Number(n)),
        Scalar::Text(t) => deferred(OwnedDeferredText::Text(t)),
        Scalar::Missing => deferred(OwnedDeferredText::Missing),
    }
}

/// Range policy for text indexes/counts before the consumer applies its own sign rules.
#[derive(Clone, Copy, Debug)]
pub enum TextIntegerPolicy {
    /// Validate the truncated integer target; overflow is VALUE.
    Checked,
    /// Preserve source floor-before-range admission and NUM on overflow.
    PinnedPoi,
}
/// Admit a signed-32 text integer argument while preserving its fractional value.
/// Sign/minimum checks and truncation remain ordered by the consuming text algorithm.
/// In particular, an empty-text shortcut must not bypass this target-range check.
pub fn coercev2_text_integer_number(
    number: f64,
    policy: TextIntegerPolicy,
) -> Result<f64, CellError> {
    let candidate = match policy {
        TextIntegerPolicy::Checked => number.trunc(),
        TextIntegerPolicy::PinnedPoi => number.floor(),
    };
    if !candidate.is_finite() || candidate < f64::from(i32::MIN) || candidate > f64::from(i32::MAX)
    {
        Err(match policy {
            TextIntegerPolicy::Checked => CellError::Value,
            TextIntegerPolicy::PinnedPoi => CellError::Num,
        })
    } else {
        Ok(number)
    }
}

/// Admit a documented FIND/SEARCH start as a zero-based position in the text.
/// Fractional starts truncate before the last-character bound. The text source
/// supplies its own character/unit length; pinned source domains stay separate.
pub fn coercev2_text_search_start(number: f64, length: usize) -> Result<usize, CellError> {
    let number = coercev2_text_integer_number(number, TextIntegerPolicy::Checked)?.trunc();
    if number < 1. || number > length as f64 {
        return Err(CellError::Value);
    }
    (number as usize).checked_sub(1).ok_or(CellError::Value)
}

/// VALUE consumes numeric/blank inputs without rendering them first, rejects
/// booleans and requests admitted Unicode projection for existing text.
pub fn coercev2_value_number<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    project: impl FnOnce(TextRef<'a>) -> Result<f64, E>,
) -> Result<f64, E> {
    match value {
        ScalarRef::Number(n) if n.is_finite() => Ok(n),
        ScalarRef::Number(_) => Err(CellError::Num.into()),
        ScalarRef::Blank => Ok(0.),
        ScalarRef::Text(t) => project(t),
        ScalarRef::Error(e) => Err(e.into()),
        _ => Err(CellError::Value.into()),
    }
}

/// Date serial admission selected by the consuming date function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateSerialPolicy {
    /// EDATE's pinned operand accepts only existing numbers and blank.
    PinnedNumberOrBlank,
    /// Modern arguments request finite scalar numbers and calendar text.
    Calendar,
}
/// Request a date serial while leaving text, context and resource effects to
/// the host continuation. Existing scalar errors are preserved by the modern policy.
pub fn coercev2_date_serial<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    policy: DateSerialPolicy,
    deferred: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<f64, E> {
    if policy == DateSerialPolicy::PinnedNumberOrBlank
        && !matches!(value, ScalarRef::Number(_) | ScalarRef::Blank)
    {
        return Err(CellError::Value.into());
    }
    coercev2_number(value, NumberPolicy::Finite, deferred)
}
/// WEEKDAY's modern option retains conversion errors; valid numeric options
/// are truncated before the calendar algorithm checks the documented modes.
pub fn coercev2_weekday_option<'a, E: From<CellError>>(
    value: ScalarRef<'a>,
    deferred: impl FnMut(DeferredNumber<'a>) -> Result<f64, E>,
) -> Result<i64, E> {
    coercev2_date_integer(coercev2_number(value, NumberPolicy::Finite, deferred)?)
        .map_err(|_| CellError::Num.into())
}
