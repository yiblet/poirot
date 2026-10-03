//! Typed lookup into an immutable generated Unicode15 asset, format revision 1.
//! Copyright © 1991-2022 Unicode, Inc. See UNICODE-LICENSE.txt alongside the asset.
const DATA: &[u8] = include_bytes!("java_case_unicode15.bin");
const HEADER: usize = 12;
const RECORD: usize = 30;
const MAGIC: &[u8] = b"ROIUC15\0";

/// Integrity/layout failure while reading the pinned full-case mapping tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InvalidAsset;
/// Borrowed lowercase/uppercase expansions decoded from a validated mapping asset.
#[derive(Clone, Copy, Debug)]
pub(super) struct Mapping {
    /// Lowercase code-point expansion storage.
    pub lower: [u32; 3],
    /// Uppercase code-point expansion storage.
    pub upper: [u32; 3],
    /// Number of lowercase expansion entries in use.
    pub lower_len: usize,
    /// Number of uppercase expansion entries in use.
    pub upper_len: usize,
}
fn integer(data: &[u8], offset: usize) -> Result<u32, InvalidAsset> {
    let bytes: [u8; 4] = data
        .get(offset..offset.checked_add(4).ok_or(InvalidAsset)?)
        .ok_or(InvalidAsset)?
        .try_into()
        .map_err(|_| InvalidAsset)?;
    Ok(u32::from_le_bytes(bytes))
}
fn count(data: &[u8]) -> Result<usize, InvalidAsset> {
    if data.get(..MAGIC.len()) != Some(MAGIC) {
        return Err(InvalidAsset);
    }
    let count = integer(data, 8)? as usize;
    let length = count
        .checked_mul(RECORD)
        .and_then(|size| size.checked_add(HEADER))
        .ok_or(InvalidAsset)?;
    if length != data.len() {
        return Err(InvalidAsset);
    }
    Ok(count)
}
fn record(data: &[u8], index: usize) -> Result<(u32, Mapping), InvalidAsset> {
    let offset = index
        .checked_mul(RECORD)
        .and_then(|size| size.checked_add(HEADER))
        .ok_or(InvalidAsset)?;
    let bytes = data
        .get(offset..offset.checked_add(RECORD).ok_or(InvalidAsset)?)
        .ok_or(InvalidAsset)?;
    // SAFETY: get above requires a full RECORD-byte slice (30 bytes); the local arrays each have three entries and index is in 0..3.
    let lower_len = usize::from(bytes[4]);
    // SAFETY: get above requires a full RECORD-byte slice (30 bytes); the local arrays each have three entries and index is in 0..3.
    let upper_len = usize::from(bytes[5]);
    if !(1..=3).contains(&lower_len) || !(1..=3).contains(&upper_len) {
        return Err(InvalidAsset);
    }
    let mut lower = [0; 3];
    let mut upper = [0; 3];
    for index in 0..3 {
        // SAFETY: get above requires a full RECORD-byte slice (30 bytes); the local arrays each have three entries and index is in 0..3.
        lower[index] = integer(bytes, 6 + index * 4)?;
        // SAFETY: get above requires a full RECORD-byte slice (30 bytes); the local arrays each have three entries and index is in 0..3.
        upper[index] = integer(bytes, 18 + index * 4)?;
    }
    Ok((
        integer(bytes, 0)?,
        Mapping {
            lower,
            upper,
            lower_len,
            upper_len,
        },
    ))
}
fn lookup_in(data: &[u8], code_point: u32) -> Result<Option<Mapping>, InvalidAsset> {
    let mut low = 0;
    let mut high = count(data)?;
    while low < high {
        let middle = low + (high - low) / 2;
        let (key, mapping) = record(data, middle)?;
        match key.cmp(&code_point) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => return Ok(Some(mapping)),
        }
    }
    Ok(None)
}
/// Read the pinned full-case expansion for one code point.
pub(super) fn lookup(code_point: u32) -> Result<Option<Mapping>, InvalidAsset> {
    lookup_in(DATA, code_point)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_asset_has_sorted_scalar_keys_and_valid_mappings() {
        let mut previous = None;
        for index in 0..count(DATA).unwrap() {
            let (key, mapping) = record(DATA, index).unwrap();
            if let Some(previous) = previous {
                assert!(previous < key);
            }
            assert!(char::from_u32(key).is_some());
            for (points, len) in [
                (mapping.lower, mapping.lower_len),
                (mapping.upper, mapping.upper_len),
            ] {
                assert!(
                    points[..len]
                        .iter()
                        .all(|&point| char::from_u32(point).is_some())
                );
                assert!(points[len..].iter().all(|&point| point == 0));
                let output_units: usize = points[..len]
                    .iter()
                    .map(|&point| if point > 0xffff { 2 } else { 1 })
                    .sum();
                let input_units = if key > 0xffff { 2 } else { 1 };
                assert!(output_units <= 3 * input_units);
            }
            previous = Some(key);
        }
        assert_eq!(lookup(0xdf).unwrap().unwrap().upper[..2], [0x53, 0x53]);
        assert!(lookup(0x1f600).unwrap().is_none());
    }
    #[test]
    fn malformed_asset_is_a_typed_failure() {
        assert!(lookup_in(b"wrong", 0xdf).is_err());
        assert!(lookup_in(&DATA[..DATA.len() - 1], 0xdf).is_err());
        let mut invalid = DATA.to_vec();
        invalid[HEADER + 4] = 4;
        assert!(record(&invalid, 0).is_err());
    }
}
