//! Count, slice, and compare text for spreadsheet functions.
//!
//! Pass [`Units`](crate::text_units::Units) directly
//! to helpers such as [`length`](crate::text_units::length) and [`slice()`](crate::text_units::slice). Supported units include UTF-8 bytes,
//! UTF-16 code units, and Unicode scalar values. Text is not normalized.
//!
//! [`fold`](crate::text_units::fold) provides Unicode 16 simple case-insensitive comparison without character
//! expansion. Its UTF-16 adapter preserves unpaired surrogates. Spreadsheet text
//! function implementations live in
//! [text](https://docs.rs/poirot-functions/latest/poirot_functions/text).

mod text_semantics_error;
pub use text_semantics_error::TextSemanticsError;

/// Unicode simple folding for case-insensitive values, independent of text indexing.
pub mod fold;

/// Index/count unit selected by a text source policy: bytes, UTF-16 units,
/// Unicode scalar values, or adjacent UTF-16 surrogate pairs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    /// Count storage bytes; slicing still requires valid UTF-8 boundaries.
    Utf8Bytes,
    /// Count Java/Excel UTF-16 code units.
    Utf16CodeUnits,
    /// Count Rust Unicode scalar values without normalization.
    UnicodeScalars,
    /// Adjacent UTF-16 surrogate pairs count once; isolated units count once.
    Utf16SurrogatePairs,
}
/// The counting/indexing unit and optional semantic length ceiling for a text operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationPolicy {
    /// Counting/indexing units for this source operation.
    pub units: Units,
    /// Source semantic ceiling, distinct from the caller's byte budget.
    pub limit: Option<usize>,
}
/// Measure borrowed UTF-8 text using the selected source units.
pub fn length(text: &str, units: Units) -> usize {
    match units {
        Units::Utf8Bytes => text.len(),
        Units::Utf16CodeUnits => text.encode_utf16().count(),
        Units::UnicodeScalars | Units::Utf16SurrogatePairs => text.chars().count(),
    }
}
/// Preflight result for concatenation under the selected text-unit policy.
/// Hosts use the byte count for allocation admission and the logical unit count to
/// check the source operation's semantic length limit before building output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConcatMeasure {
    /// UTF-8 storage required for concatenated output.
    pub bytes: usize,
    /// Logical length under the selected source counting units.
    pub source_units: usize,
    /// Whether logical length meets the source semantic ceiling.
    pub within_limit: bool,
}
/// Check total storage and logical length before building concatenated output.
pub fn measure_concat<'a>(
    parts: impl IntoIterator<Item = &'a str>,
    policy: OperationPolicy,
) -> Result<ConcatMeasure, TextSemanticsError> {
    let mut bytes = 0usize;
    let mut source_units = 0usize;
    for part in parts {
        bytes = checked_total(bytes, part.len())?;
        source_units = checked_total(source_units, length(part, policy.units))?;
    }
    Ok(ConcatMeasure {
        bytes,
        source_units,
        within_limit: policy.limit.is_none_or(|limit| source_units <= limit),
    })
}
/// Accumulate text sizes without allowing arithmetic overflow.
pub fn checked_total(total: usize, additional: usize) -> Result<usize, TextSemanticsError> {
    total
        .checked_add(additional)
        .ok_or(TextSemanticsError::SizeOverflow)
}

/// Zero-based range, clamped to the source length. Numeric coercion and Excel's
/// one-based argument validation remain the caller's responsibility.
pub fn slice(
    text: &str,
    start: usize,
    count: usize,
    units: Units,
) -> Result<&str, TextSemanticsError> {
    let len = length(text, units);
    let start = start.min(len);
    let end = start.saturating_add(count).min(len);
    // Empty slices do not expose the contents of a split surrogate pair.
    if start == end {
        return Ok("");
    }
    let begin = boundary(text, start, units)?;
    let end = boundary(text, end, units)?;
    text.get(begin..end)
        .ok_or(TextSemanticsError::NonScalarByteBoundary)
}
/// Borrow a prefix measured in the selected source units.
pub fn left(text: &str, count: usize, units: Units) -> Result<&str, TextSemanticsError> {
    slice(text, 0, count, units)
}
/// Borrow a suffix measured in the selected source units.
pub fn right(text: &str, count: usize, units: Units) -> Result<&str, TextSemanticsError> {
    let start = length(text, units).saturating_sub(count);
    slice(text, start, count, units)
}
fn boundary(text: &str, index: usize, units: Units) -> Result<usize, TextSemanticsError> {
    if units == Units::Utf8Bytes {
        return if text.is_char_boundary(index) {
            Ok(index)
        } else {
            Err(TextSemanticsError::NonScalarByteBoundary)
        };
    }
    let mut offset = 0;
    for (byte, character) in text.char_indices() {
        if offset == index {
            return Ok(byte);
        }
        offset += if units == Units::Utf16CodeUnits {
            character.len_utf16()
        } else {
            1
        };
        if offset > index {
            return Err(TextSemanticsError::UnpairedSurrogateResult);
        }
    }
    Ok(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_units_are_not_display_width_or_normalization() {
        let text = "A😀e\u{301}";
        assert_eq!(length(text, Units::Utf8Bytes), 8);
        assert_eq!(length(text, Units::Utf16CodeUnits), 5);
        assert_eq!(length(text, Units::UnicodeScalars), 4);
        assert_eq!(slice(text, 3, 2, Units::Utf16CodeUnits), Ok("e\u{301}"));
        assert_ne!(
            length("e\u{301}", Units::UnicodeScalars),
            length("é", Units::UnicodeScalars)
        );
    }

    #[test]
    fn paired_units_preserve_combining_characters() {
        assert_eq!(
            left("😀e\u{301}\u{fe0f}", 1, Units::Utf16SurrogatePairs),
            Ok("😀")
        );
        assert_eq!(length("😀e\u{301}\u{fe0f}", Units::Utf16SurrogatePairs), 4);
    }
}
