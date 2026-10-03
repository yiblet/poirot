//! UnicodeData simple mappings; Unicode notice is in UNICODE-LICENSE.txt.
use std::sync::OnceLock;
const DATA: &[u8] = include_bytes!("java_simple_case_unicode15.bin");
const MAGIC: &[u8] = b"ROIS15\0\0";
const HEADER: usize = 12;
const RECORD: usize = 12;
/// Validation failure for the pinned Java simple-case mapping asset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InvalidAsset;
fn integer(data: &[u8], offset: usize) -> Result<u32, InvalidAsset> {
    let bytes = data
        .get(offset..offset.checked_add(4).ok_or(InvalidAsset)?)
        .ok_or(InvalidAsset)?;
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| InvalidAsset)?,
    ))
}
fn count(data: &[u8]) -> Result<usize, InvalidAsset> {
    if data.get(..8) != Some(MAGIC) {
        return Err(InvalidAsset);
    }
    let n = integer(data, 8)? as usize;
    if n.checked_mul(RECORD).and_then(|n| n.checked_add(HEADER)) != Some(data.len()) {
        return Err(InvalidAsset);
    }
    Ok(n)
}
fn row(data: &[u8], index: usize) -> Result<[u32; 3], InvalidAsset> {
    let offset = index
        .checked_mul(RECORD)
        .and_then(|i| i.checked_add(HEADER))
        .ok_or(InvalidAsset)?;
    Ok([
        integer(data, offset)?,
        integer(data, offset + 4)?,
        integer(data, offset + 8)?,
    ])
}
fn scalar(cp: u32) -> bool {
    cp <= 0x10ffff && !(0xd800..=0xdfff).contains(&cp)
}
fn validate(data: &[u8]) -> Result<(), InvalidAsset> {
    let mut previous = None;
    for i in 0..count(data)? {
        let [key, lower, upper] = row(data, i)?;
        if !scalar(key)
            || !scalar(lower)
            || !scalar(upper)
            || previous.is_some_and(|p| p >= key)
            || (key == lower && key == upper)
        {
            return Err(InvalidAsset);
        }
        previous = Some(key);
    }
    Ok(())
}
/// Validate the simple-case mapping asset before exposing comparisons.
pub(super) fn admit() -> Result<(), InvalidAsset> {
    static VALID: OnceLock<Result<(), InvalidAsset>> = OnceLock::new();
    *VALID.get_or_init(|| validate(DATA))
}
/// Apply the pinned code-point simple-case mapping.
pub(super) fn map(cp: u32, upper: bool) -> Result<u32, InvalidAsset> {
    let mut low = 0;
    let mut high = count(DATA)?;
    while low < high {
        let middle = low + (high - low) / 2;
        let [key, lower, uppercase] = row(DATA, middle)?;
        match key.cmp(&cp) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return Ok(if upper { uppercase } else { lower }),
        }
    }
    Ok(cp)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_asset_admission_precedes_lookup() {
        assert_eq!(admit(), Ok(()));
        assert!(validate(b"bad").is_err());
        assert!(validate(&DATA[..DATA.len() - 1]).is_err());
        let mut corrupt = DATA.to_vec();
        corrupt[HEADER..HEADER + 4].copy_from_slice(&0xd800u32.to_le_bytes());
        assert!(validate(&corrupt).is_err());
        let mut corrupt = DATA.to_vec();
        corrupt[HEADER + RECORD..HEADER + 2 * RECORD]
            .copy_from_slice(&DATA[HEADER..HEADER + RECORD]);
        assert!(validate(&corrupt).is_err());
        assert_eq!(map(0x130, false), Ok(0x69));
        assert_eq!(map(0xdf, true), Ok(0xdf));
    }
}
