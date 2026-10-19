//! Unicode15 ROOT case mappings with Unicode contextual final sigma.
//! Mapping assets retain the Java-pinned Unicode input version, while sigma
//! follows Unicode15 SpecialCasing/section 3.13 rather than Java word boundaries.
#[path = "case_properties.rs"]
mod case_properties;
#[path = "java_case_data.rs"]
mod data;

/// Version of the pinned full-case mapping contract and assets.
pub const POLICY_VERSION: &str = "unicode15-root-final-sigma-v2";

/// Requested Unicode15 full-case mapping direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Apply lowercase mapping.
    Lower,
    /// Apply uppercase mapping.
    Upper,
}
/// Case-mapping asset or context gaps reported without substituting Rust Unicode rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// Pinned mapping asset failed integrity or layout validation.
    InvalidMappingAsset,
}

/// Maximum output length in UTF-16 units (all Unicode 15 expansions <= 3 units
/// per input unit). Hosts should reserve/check this before invoking conversion.
pub fn output_bound(input_units: usize) -> Option<usize> {
    input_units.checked_mul(3)
}

/// Preserve unpaired surrogates exactly, as Java String does.
pub fn convert(input: &[u16], direction: Direction) -> Result<Vec<u16>, Unsupported> {
    let points = code_points(input);
    let mut output = Vec::new();
    for (index, &(cp, _)) in points.iter().enumerate() {
        if direction == Direction::Lower && cp == 0x3a3 {
            push_code_point(
                &mut output,
                if final_sigma(&points, index) {
                    0x3c2
                } else {
                    0x3c3
                },
            );
        } else if let Some(mapping) =
            data::lookup(cp).map_err(|_| Unsupported::InvalidMappingAsset)?
        {
            let mapping = match direction {
                Direction::Lower => mapping
                    .lower
                    .get(..mapping.lower_len)
                    .ok_or(Unsupported::InvalidMappingAsset)?,
                Direction::Upper => mapping
                    .upper
                    .get(..mapping.upper_len)
                    .ok_or(Unsupported::InvalidMappingAsset)?,
            };
            for &mapped in mapping {
                push_code_point(&mut output, mapped);
            }
        } else {
            push_code_point(&mut output, cp);
        }
    }
    Ok(output)
}

/// Java String.indexOf, including empty needles and UTF-16 offsets. A negative
/// Java fromIndex is represented by 0; callers validate POI's start_num first.
pub fn index_of(haystack: &[u16], needle: &[u16], from_index: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from_index.min(haystack.len()));
    }
    if from_index > haystack.len() || needle.len() > haystack.len() - from_index {
        return None;
    }
    // SAFETY: The preceding guard checks from_index <= haystack.len(); the earlier branch returns for an empty needle.
    haystack[from_index..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| position + from_index)
}

/// POI SEARCH uppercases both strings, then searches literally. Returned offsets
/// refer to the expanded uppercase haystack, not the original string.
pub fn search(
    haystack: &[u16],
    needle: &[u16],
    from_index: usize,
) -> Result<Option<usize>, Unsupported> {
    let haystack = convert(haystack, Direction::Upper)?;
    let needle = convert(needle, Direction::Upper)?;
    Ok(index_of(&haystack, &needle, from_index))
}

fn code_points(input: &[u16]) -> Vec<(u32, usize)> {
    let mut points = Vec::new();
    let mut offset = 0;
    while offset < input.len() {
        // SAFETY: The loop checks offset < input.len(); the surrogate branch also checks input.get(offset + 1) is Some.
        let first = input[offset];
        if (0xd800..=0xdbff).contains(&first)
            && input
                .get(offset + 1)
                .is_some_and(|next| (0xdc00..=0xdfff).contains(next))
        {
            // SAFETY: The loop checks offset < input.len(); the surrogate branch also checks input.get(offset + 1) is Some.
            let second = input[offset + 1];
            points.push((
                0x10000 + ((u32::from(first) - 0xd800) << 10) + u32::from(second) - 0xdc00,
                offset,
            ));
            offset += 2;
        } else {
            points.push((u32::from(first), offset));
            offset += 1;
        }
    }
    points
}
fn push_code_point(output: &mut Vec<u16>, cp: u32) {
    if cp <= 0xffff {
        output.push(cp as u16);
    } else {
        let cp = cp - 0x10000;
        output.push((0xd800 + (cp >> 10)) as u16);
        output.push((0xdc00 + (cp & 0x3ff)) as u16);
    }
}

/// Test a generated Unicode property without depending on Rust's Unicode version.
fn has_property(cp: u32, ranges: &[(u32, u32)]) -> bool {
    let index = ranges.partition_point(|&(start, _)| start <= cp);
    index
        .checked_sub(1)
        .and_then(|i| ranges.get(i))
        .is_some_and(|&(_, end)| cp <= end)
}

/// Apply Unicode15 section 3.13 Final_Sigma to the original input context.
fn final_sigma(points: &[(u32, usize)], index: usize) -> bool {
    // Unicode SpecialCasing 15.0.0, U+03A3 Final_Sigma; Unicode data license.
    // https://www.unicode.org/Public/15.0.0/ucd/SpecialCasing.txt
    // A preceding cased character is required; following cased characters prevent
    // final form. Unicode table 3-17 uses possessive Case_Ignorable runs:
    // overlapping Cased+Case_Ignorable characters are skipped, not reconsidered.
    let before = points.get(..index).and_then(|prefix| {
        prefix
            .iter()
            .rev()
            .find(|&&(cp, _)| !has_property(cp, case_properties::CASE_IGNORABLE))
    });
    let after = index
        .checked_add(1)
        .and_then(|start| points.get(start..))
        .and_then(|suffix| {
            suffix
                .iter()
                .find(|&&(cp, _)| !has_property(cp, case_properties::CASE_IGNORABLE))
        });
    before.is_some_and(|&(cp, _)| has_property(cp, case_properties::CASED))
        && !after.is_some_and(|&(cp, _)| has_property(cp, case_properties::CASED))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn check(text: &str, direction: Direction, expected: &str) {
        assert_eq!(convert(&units(text), direction), Ok(units(expected)));
    }
    #[test]
    fn source_pinned_full_mappings() {
        check("ABC xyz", Direction::Lower, "abc xyz");
        check("Straße µ ÿ ﬃ", Direction::Upper, "STRASSE Μ Ÿ FFI");
        check("Iİıi", Direction::Lower, "ii\u{307}ıi");
        check("Iİıi", Direction::Upper, "IİII");
        check("\u{10400}\u{10428}", Direction::Lower, "\u{10428}\u{10428}");
        check("\u{10400}\u{10428}", Direction::Upper, "\u{10400}\u{10400}");
    }
    #[test]
    fn sigma_uses_original_word_context() {
        check(
            "ΟΣ ΟΣΑ ΣΟΣ Σ AΣ1 AΣ\u{301}",
            Direction::Lower,
            "ος οσα σος σ aς1 aς\u{301}",
        );
        check("A.Σ", Direction::Lower, "a.ς");
        check("AΣ1", Direction::Lower, "aς1");
        check("AΣ1A", Direction::Lower, "aς1a");
        check("AΣ\u{301}A", Direction::Lower, "aσ\u{301}a");
        check("Σ", Direction::Lower, "σ");
        // Unicode table 3-17 explicitly identifies U+0345 as both properties.
        check("\u{345}Σ", Direction::Lower, "\u{345}σ");
        check("A\u{345}Σ", Direction::Lower, "a\u{345}ς");
        check("AΣ\u{345}", Direction::Lower, "aς\u{345}");
        check("AΣ\u{345}A", Direction::Lower, "aσ\u{345}a");
        check("A-Σ", Direction::Lower, "a-σ");
        // Unconditional mappings remain independent of sigma context.
        check("A.B", Direction::Lower, "a.b");
    }
    #[test]
    fn search_is_literal_and_measures_expanded_utf16() {
        assert_eq!(search(&units("a*b"), &units("*"), 0), Ok(Some(1)));
        assert_eq!(search(&units("a?b"), &units("?"), 0), Ok(Some(1)));
        assert_eq!(search(&units("Straße"), &units("ß"), 0), Ok(Some(4)));
        assert_eq!(search(&units("ßX"), &units("x"), 0), Ok(Some(2)));
        assert_eq!(search(&units("😀x"), &units("X"), 0), Ok(Some(2)));
        assert_eq!(search(&units("abc"), &[], 3), Ok(Some(3)));
        assert_eq!(index_of(&units("abc"), &[], 10), Some(3));
        assert_eq!(index_of(&units("abc"), &units("a"), 4), None);
    }
    #[test]
    fn surrogate_units_are_preserved_and_searchable() {
        let units = [0xd800, 0x61, 0xdc00];
        assert_eq!(
            convert(&units, Direction::Upper),
            Ok(vec![0xd800, 0x41, 0xdc00])
        );
        assert_eq!(search(&units, &[0xdc00], 0), Ok(Some(2)));
        assert_eq!(output_bound(usize::MAX), None);
    }
}
