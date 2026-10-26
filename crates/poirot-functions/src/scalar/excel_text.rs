//! Lossless spreadsheet text. Indices and lengths are UTF-16 code units.
//! Case mapping and formula argument coercion belong to explicit source policies.
use std::ops::Range;

/// Lossless spreadsheet text stored as UTF-16 code units, including unpaired surrogates.
/// Text algorithms can use Excel code-unit positions without requiring conversion to a Rust string.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExcelText(Box<[u16]>);

/// Invalid unit range, one-based start, or encoded byte length in a text operation.
/// Malformed UTF-16 is allowed in [`ExcelText`]; converting it to UTF-8 can fail separately.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextUnitsError {
    /// Requested UTF-16 unit interval lies outside the stored text.
    InvalidRange,
    /// Starting UTF-16 unit position is outside the text.
    InvalidStart,
    /// UTF-16 byte input contains an incomplete code unit.
    OddByteLength,
}

impl ExcelText {
    /// Own exact UTF-16 units, preserving unpaired surrogates.
    pub fn from_units(units: impl Into<Box<[u16]>>) -> Self {
        Self(units.into())
    }
    /// Borrow the original UTF-16 code units.
    pub fn units(&self) -> &[u16] {
        &self.0
    }
    /// Count UTF-16 code units rather than Unicode scalar values.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Whether the stored code-unit sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Projection rejects malformed UTF-16; it never inserts replacement characters.
    pub fn to_utf8(&self) -> Result<String, std::string::FromUtf16Error> {
        String::from_utf16(&self.0)
    }
    /// Strict zero-based, half-open slicing. Surrogate boundaries are ordinary boundaries.
    pub fn slice(&self, range: Range<usize>) -> Result<Self, TextUnitsError> {
        self.0
            .get(range)
            .map(|units| Self(units.into()))
            .ok_or(TextUnitsError::InvalidRange)
    }
    /// Copy a prefix measured in UTF-16 code units.
    pub fn left(&self, count: usize) -> Self {
        // SAFETY: min bounds the prefix by the UTF-16 buffer length; unit slices have no character-boundary restriction.
        Self(self.0[..count.min(self.len())].into())
    }
    /// Copy a suffix measured in UTF-16 code units.
    pub fn right(&self, count: usize) -> Self {
        // SAFETY: saturating_sub keeps the suffix start between zero and the UTF-16 buffer length.
        Self(self.0[self.len().saturating_sub(count)..].into())
    }
    /// One-based MID position; counts already validated/coerced by the caller.
    /// Clamps excessive counts without overflowing host arithmetic.
    pub fn mid(&self, start: usize, count: usize) -> Result<Self, TextUnitsError> {
        let start = start.checked_sub(1).ok_or(TextUnitsError::InvalidStart)?;
        if start >= self.len() {
            return Ok(Self::default());
        }
        let end = start + count.min(self.len() - start);
        self.slice(start..end)
    }
    /// Java String.indexOf unit semantics, including empty needle past the end.
    /// Returns a zero-based position, not a spreadsheet error.
    pub fn index_of(&self, needle: &Self, start: usize) -> Option<usize> {
        if needle.is_empty() {
            return Some(start.min(self.len()));
        }
        let tail = self.0.get(start..)?;
        // SAFETY: The empty-needle branch returned above, so needle.len() is nonzero.
        tail.windows(needle.len())
            .position(|w| w == needle.units())
            .map(|position| start + position)
    }
    /// One-based FIND position; an absent match remains distinct from invalid start.
    pub fn find(&self, needle: &Self, start: usize) -> Result<Option<usize>, TextUnitsError> {
        let start = start.checked_sub(1).ok_or(TextUnitsError::InvalidStart)?;
        Ok(self.index_of(needle, start).map(|position| position + 1))
    }
    /// BOM-free UTF-16BE evidence format used by retained native POI artifacts.
    pub fn to_utf16be(&self) -> Vec<u8> {
        self.0.iter().flat_map(|unit| unit.to_be_bytes()).collect()
    }
    /// Decode big-endian UTF-16 bytes without normalizing code units.
    pub fn from_utf16be(bytes: &[u8]) -> Result<Self, TextUnitsError> {
        // SAFETY: The chunk size is the nonzero literal 2.
        let (chunks, remainder) = bytes.as_chunks::<2>();
        let units: Vec<u16> = chunks
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        if !remainder.is_empty() {
            return Err(TextUnitsError::OddByteLength);
        }
        Ok(Self::from_units(units))
    }
}
impl From<&str> for ExcelText {
    fn from(text: &str) -> Self {
        Self::from_units(text.encode_utf16().collect::<Vec<_>>())
    }
}
impl From<String> for ExcelText {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_pairs_and_ordinary_units() {
        let text = ExcelText::from("A😀Z");
        assert_eq!(text.units(), &[65, 0xd83d, 0xde00, 90]);
        assert_eq!(text.len(), 4);
        assert_eq!(text.to_utf8().unwrap(), "A😀Z");
        assert_eq!(text.mid(2, 2).unwrap().to_utf8().unwrap(), "😀");
        assert_eq!(text.left(usize::MAX), text);
        assert_eq!(text.right(usize::MAX), text);
    }
    #[test]
    fn split_and_lone_surrogates_are_values() {
        let emoji = ExcelText::from("😀");
        let high = emoji.left(1);
        let low = emoji.right(1);
        assert_eq!(high.units(), &[0xd83d]);
        assert_eq!(low.units(), &[0xde00]);
        assert!(high.to_utf8().is_err());
        assert!(low.to_utf8().is_err());
        assert_eq!(emoji.mid(2, 1).unwrap(), low);
        for units in [vec![0xd800], vec![0xdc00], vec![0xdc00, 65, 0xd800]] {
            let text = ExcelText::from_units(units);
            assert!(text.to_utf8().is_err());
            assert_eq!(ExcelText::from_utf16be(&text.to_utf16be()).unwrap(), text);
        }
    }
    #[test]
    fn find_indexes_units_including_inside_pairs() {
        let text = ExcelText::from("A😀Z😀");
        assert_eq!(text.find(&ExcelText::from("Z"), 1), Ok(Some(4)));
        assert_eq!(text.find(&ExcelText::from("😀"), 3), Ok(Some(5)));
        assert_eq!(
            text.find(&ExcelText::from_units(vec![0xde00]), 1),
            Ok(Some(3))
        );
        assert_eq!(text.find(&ExcelText::from("Q"), 1), Ok(None));
        assert_eq!(text.find(&ExcelText::default(), 100), Ok(Some(7)));
        assert_eq!(text.find(&ExcelText::from("A"), 100), Ok(None));
        assert_eq!(
            text.find(&ExcelText::default(), 0),
            Err(TextUnitsError::InvalidStart)
        );
    }
    #[test]
    fn bounds_and_serialized_evidence() {
        let text = ExcelText::from("AB");
        assert_eq!(text.slice(2..2).unwrap(), ExcelText::default());
        let reversed_start = text.len();
        let reversed_end = 1;
        assert_eq!(
            text.slice(reversed_start..reversed_end),
            Err(TextUnitsError::InvalidRange)
        );
        assert_eq!(text.slice(0..3), Err(TextUnitsError::InvalidRange));
        assert_eq!(text.mid(0, 1), Err(TextUnitsError::InvalidStart));
        assert!(text.mid(usize::MAX, usize::MAX).unwrap().is_empty());
        assert_eq!(text.mid(2, usize::MAX).unwrap(), ExcelText::from("B"));
        assert_eq!(
            ExcelText::from_utf16be(&[0xd8, 0x3d]).unwrap().units(),
            &[0xd83d]
        );
        assert_eq!(
            ExcelText::from_utf16be(&[0xde, 0x00]).unwrap().units(),
            &[0xde00]
        );
        assert_eq!(
            ExcelText::from_utf16be(&[0]),
            Err(TextUnitsError::OddByteLength)
        );
        assert_eq!(ExcelText::from("\u{fffd}").to_utf8().unwrap(), "\u{fffd}");
    }
}
