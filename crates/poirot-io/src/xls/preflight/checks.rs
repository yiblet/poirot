//! Checked little-endian integer reads and resource-limit comparisons.
use super::preflight_error::PreflightError;

pub(super) fn check(
    resource: crate::BiffResource,
    requested: u64,
    limit: u64,
) -> Result<(), PreflightError> {
    if requested > limit {
        Err(PreflightError::Resource {
            resource,
            requested,
            limit,
        })
    } else {
        Ok(())
    }
}

pub(super) fn u16at(b: &[u8], i: usize) -> Result<u16, PreflightError> {
    let s = b
        .get(
            i..i.checked_add(2).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::IntegerOffsetOverflow,
            ))?,
        )
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::TruncatedInteger,
        ))?;
    // SAFETY: The preceding get requires an exactly two-byte slice before either byte is selected.
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

pub(super) fn u32at(b: &[u8], i: usize) -> Result<u32, PreflightError> {
    let s = b
        .get(
            i..i.checked_add(4).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::IntegerOffsetOverflow,
            ))?,
        )
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::TruncatedInteger,
        ))?;
    // SAFETY: The preceding get requires an exactly four-byte slice, so conversion to [u8; 4] succeeds.
    Ok(u32::from_le_bytes(s.try_into().unwrap()))
}
