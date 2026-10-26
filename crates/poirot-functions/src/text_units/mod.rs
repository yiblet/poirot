//! Count, slice, and compare text for spreadsheet functions.
//!
//! Use [`TextSemanticsPolicy`](crate::text_units::TextSemanticsPolicy) to select text-counting rules, or pass [`Units`](crate::text_units::Units) directly
//! to helpers such as [`length`](crate::text_units::length) and [`slice()`](crate::text_units::slice). Supported units include UTF-8 bytes,
//! UTF-16 code units, and Unicode scalar values. Text is not normalized.
//!
//! [`fold`](crate::text_units::fold) provides Unicode 16 simple case-insensitive comparison without character
//! expansion. Its UTF-16 adapter preserves unpaired surrogates. Spreadsheet text
//! function implementations live in
//! [text](https://docs.rs/poirot-functions/latest/poirot_functions/text).

/// Unicode simple folding for case-insensitive values, independent of text indexing.
pub mod fold;

/// Sources are explicit. No authentic modern Excel observations are available.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SourceVersion {
    /// Text counting and indexing from the pinned POI revision.
    Poi7321209,
    /// Pinned Unicode15 tables and Unicode final sigma; no Java runtime parity claim.
    Poi7321209OpenJdk21Unicode15,
    /// Microsoft documented surrogate-pair counting; not observed Excel parity.
    ExcelCompatibilityV2Documented,
    /// Unverified Excel contract; execution refuses rather than inferring behavior.
    ExcelCompatibilityV2Unverified,
}
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
/// Text operation whose indexing and length rules are selected by [`TextSemanticsPolicy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Length measurement.
    Len,
    /// Substring indexing and selection.
    Index,
    /// Binary ampersand concatenation.
    ConcatOperator,
    /// Legacy CONCATENATE function.
    Concatenate,
    /// CONCAT function.
    Concat,
}
/// A text operation that cannot be represented under the selected rules.
/// These failures are distinct from spreadsheet `#VALUE!` errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// Requested Excel compatibility version lacks an admitted contract.
    UnverifiedExcelVersion,
    /// Selected UTF-16 units cannot be returned as borrowed UTF-8.
    UnpairedSurrogateResult,
    /// Requested byte boundary splits a UTF-8 character.
    NonScalarByteBoundary,
    /// Text-size arithmetic overflowed before allocation.
    SizeOverflow,
    /// Required Java Unicode case mapping is unavailable.
    JavaUnicodeCaseMapping,
    /// Selected source revision does not define this operation.
    MissingSourceOperation,
    /// Pinned Java signed-index arithmetic overflowed.
    JavaIndexOverflow,
}
/// The counting/indexing unit and optional semantic length ceiling for a text operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationPolicy {
    /// Counting/indexing units for this source operation.
    pub units: Units,
    /// Source semantic ceiling, distinct from the caller's byte budget.
    pub limit: Option<usize>,
}
/// Explicit source baseline for formula text counting, indexing, and construction.
/// A documented Excel policy is not an observed compatibility guarantee; unsupported
/// source/version combinations are refused rather than guessed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TextSemanticsPolicy {
    /// Explicit source/version contract used for text operations.
    pub source: SourceVersion,
}
impl Default for TextSemanticsPolicy {
    fn default() -> Self {
        Self {
            source: SourceVersion::ExcelCompatibilityV2Documented,
        }
    }
}
impl TextSemanticsPolicy {
    /// Resolve indexing units and semantic length limits for one operation.
    pub fn operation(self, operation: Operation) -> Result<OperationPolicy, Unsupported> {
        use Operation::*;
        let (units, limit) = match (self.source, operation) {
            (SourceVersion::ExcelCompatibilityV2Unverified, _) => {
                return Err(Unsupported::UnverifiedExcelVersion);
            }
            (SourceVersion::ExcelCompatibilityV2Documented, Len | Index) => {
                (Units::Utf16SurrogatePairs, None)
            }
            (SourceVersion::ExcelCompatibilityV2Documented, _) => (Units::Utf16CodeUnits, None),
            (SourceVersion::Poi7321209 | SourceVersion::Poi7321209OpenJdk21Unicode15, _) => {
                (Units::Utf16CodeUnits, None)
            }
        };
        Ok(OperationPolicy { units, limit })
    }
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
) -> Result<ConcatMeasure, Unsupported> {
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
pub fn checked_total(total: usize, additional: usize) -> Result<usize, Unsupported> {
    total
        .checked_add(additional)
        .ok_or(Unsupported::SizeOverflow)
}

/// Zero-based range, clamped to the source length. Numeric coercion and Excel's
/// one-based argument validation remain the caller's responsibility.
pub fn slice(text: &str, start: usize, count: usize, units: Units) -> Result<&str, Unsupported> {
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
        .ok_or(Unsupported::NonScalarByteBoundary)
}
/// Borrow a prefix measured in the selected source units.
pub fn left(text: &str, count: usize, units: Units) -> Result<&str, Unsupported> {
    slice(text, 0, count, units)
}
/// Borrow a suffix measured in the selected source units.
pub fn right(text: &str, count: usize, units: Units) -> Result<&str, Unsupported> {
    let start = length(text, units).saturating_sub(count);
    slice(text, start, count, units)
}
/// POI MID's signed addition can overflow before Math.min; do not silently
/// substitute saturating Rust arithmetic for that Java source fault.
/// Requires already validated start >= 1 and count >= 0.
pub fn poi_mid(text: &str, start: i32, count: i32) -> Result<&str, Unsupported> {
    let start = start
        .checked_sub(1)
        .filter(|n| *n >= 0)
        .ok_or(Unsupported::JavaIndexOverflow)?;
    if count < 0 {
        return Err(Unsupported::JavaIndexOverflow);
    }
    if start as usize > length(text, Units::Utf16CodeUnits) {
        return Ok("");
    }
    start
        .checked_add(count)
        .ok_or(Unsupported::JavaIndexOverflow)?;
    slice(text, start as usize, count as usize, Units::Utf16CodeUnits)
}
fn boundary(text: &str, index: usize, units: Units) -> Result<usize, Unsupported> {
    if units == Units::Utf8Bytes {
        return if text.is_char_boundary(index) {
            Ok(index)
        } else {
            Err(Unsupported::NonScalarByteBoundary)
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
            return Err(Unsupported::UnpairedSurrogateResult);
        }
    }
    Ok(text.len())
}

/// Java Locale.ROOT case mapping requires a pinned Java Unicode implementation.
/// ASCII is version-independent; non-ASCII is explicitly unsupported here.
pub fn java_root_uppercase(text: &str) -> Result<String, Unsupported> {
    if !text.is_ascii() {
        return Err(Unsupported::JavaUnicodeCaseMapping);
    }
    Ok(text.to_ascii_uppercase())
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
    fn surrogate_splits_are_explicit() {
        let u = Units::Utf16CodeUnits;
        assert_eq!(left("😀", 1, u), Err(Unsupported::UnpairedSurrogateResult));
        assert_eq!(right("😀", 1, u), Err(Unsupported::UnpairedSurrogateResult));
        assert_eq!(
            slice("A😀B", 2, 1, u),
            Err(Unsupported::UnpairedSurrogateResult)
        );
        assert_eq!(slice("😀", 1, 0, u), Ok(""));
        assert_eq!(slice("😀", usize::MAX, usize::MAX, u), Ok(""));
        assert_eq!(left("😀", usize::MAX, u), Ok("😀"));
        assert_eq!(right("A😀B", 3, u), Ok("😀B"));
        assert_eq!(
            poi_mid("abc", 2, i32::MAX),
            Err(Unsupported::JavaIndexOverflow)
        );
        assert_eq!(poi_mid("abc", i32::MAX, i32::MAX), Ok(""));
    }
    #[test]
    fn source_limits_and_allocation_units_are_separate() {
        let policy = TextSemanticsPolicy {
            source: SourceVersion::Poi7321209,
        };
        let text = "😀".repeat(8192);
        assert!(
            measure_concat(
                [text.as_str()],
                policy.operation(Operation::ConcatOperator).unwrap()
            )
            .unwrap()
            .within_limit
        );
        assert!(
            measure_concat(
                [text.as_str()],
                policy.operation(Operation::Concatenate).unwrap()
            )
            .unwrap()
            .within_limit
        );
        let p = OperationPolicy {
            units: Units::Utf16CodeUnits,
            limit: Some(32767),
        };
        assert!(
            measure_concat(["a".repeat(32767).as_str()], p)
                .unwrap()
                .within_limit
        );
        assert!(
            !measure_concat(["a".repeat(32768).as_str()], p)
                .unwrap()
                .within_limit
        );
        assert_eq!(checked_total(usize::MAX, 1), Err(Unsupported::SizeOverflow));
        let poi = TextSemanticsPolicy {
            source: SourceVersion::Poi7321209,
        };
        assert_eq!(poi.operation(Operation::Concat).unwrap().limit, None);
    }
    #[test]
    fn documented_pair_policy_is_distinct_from_scalar_and_unverified_sources() {
        let policy = TextSemanticsPolicy {
            source: SourceVersion::ExcelCompatibilityV2Documented,
        };
        for operation in [Operation::Len, Operation::Index] {
            assert_eq!(
                policy.operation(operation).unwrap().units,
                Units::Utf16SurrogatePairs
            );
        }
        assert_eq!(
            left("😀e\u{301}\u{fe0f}", 1, Units::Utf16SurrogatePairs),
            Ok("😀")
        );
        assert_eq!(length("😀e\u{301}\u{fe0f}", Units::Utf16SurrogatePairs), 4);
    }
    #[test]
    fn unsupported_sources_and_case_do_not_guess() {
        let excel = TextSemanticsPolicy {
            source: SourceVersion::ExcelCompatibilityV2Unverified,
        };
        assert_eq!(
            excel.operation(Operation::Len),
            Err(Unsupported::UnverifiedExcelVersion)
        );
        assert_eq!(
            java_root_uppercase("ßİΣς"),
            Err(Unsupported::JavaUnicodeCaseMapping)
        );
        assert_eq!(java_root_uppercase("Abc"), Ok("ABC".into()));
    }
}
