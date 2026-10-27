//! Complete borrowed record inventory, including records after logical EOF.
use super::checks::{check, u16at};
use super::preflight_error::PreflightError;
use super::record::Record;
use super::resource_budget::ResourceBudget;

/// Validate BIFF record boundaries before any payload-specific decoding.
pub(in crate::xls) fn records(
    bytes: &[u8],
    budget: ResourceBudget,
) -> Result<Vec<Record<'_>>, PreflightError> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut seen_eof = false;
    while at < bytes.len() {
        // Fixed-size Workbook streams can end with 1–3 zero padding bytes
        // after the last complete record. Do not interpret padding as a
        // truncated BIFF header; retain errors for nonzero truncated bytes.
        // SAFETY: The enclosing loop checks at < bytes.len() before taking the remaining-byte suffix.
        if bytes.len() - at < 4 && seen_eof && bytes[at..].iter().all(|b| *b == 0) {
            break;
        }
        // Compound stream padding may contain zero records; those are harmless.
        let id = u16at(bytes, at)?;
        let n = u16at(bytes, at + 2)? as usize;
        let end = at.checked_add(4 + n).ok_or(PreflightError::Malformed(
            crate::BiffStructureError::RecordOffsetOverflow,
        ))?;
        let data = bytes.get(at + 4..end).ok_or(PreflightError::Malformed(
            crate::BiffStructureError::TruncatedRecord,
        ))?;
        check(
            crate::BiffResource::BiffRecords,
            (out.len() + 1) as u64,
            budget.max_records as u64,
        )?;
        seen_eof |= id == 0xa;
        out.push(Record { id, at, data });
        at = end;
    }
    Ok(out)
}
