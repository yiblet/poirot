//! Segmented SST accounting; totals accumulate while active lookup sizes replace.
use super::checks::{check, u32at};
use super::preflight_error::PreflightError;
use super::record::Record;
use super::resource_budget::ResourceBudget;

/// Validated shared-string counts and sizes used to admit backend decoding.
#[derive(Default)]
pub(super) struct SstAccounting {
    /// Per-string storage claims recovered without constructing decoded values.
    pub(super) sizes: Vec<usize>,
    /// Number of shared strings inspected.
    pub(super) strings: usize,
    /// Combined admitted shared-string storage estimate.
    pub(super) string_bytes: usize,
}

/// Budgeted cursor over SST continuation payloads and encoding boundaries.
struct Segments<'a> {
    parts: Vec<&'a [u8]>,
    part: usize,
    at: usize,
}

impl Segments<'_> {
    fn advance(&mut self) -> bool {
        if self.part + 1 < self.parts.len() {
            self.part += 1;
            self.at = 0;
            true
        } else {
            false
        }
    }

    fn remaining(&self) -> usize {
        self.parts
            .get(self.part)
            .map_or(0, |part| part.len().saturating_sub(self.at))
    }

    fn byte(&mut self) -> Result<u8, PreflightError> {
        if self.remaining() == 0 && !self.advance() {
            return Err(PreflightError::Malformed(
                crate::BiffStructureError::TruncatedSharedString,
            ));
        }
        let b = *self
            .parts
            .get(self.part)
            .ok_or(PreflightError::Malformed(
                crate::BiffStructureError::MissingStringContinuation,
            ))?
            .get(self.at)
            .ok_or(PreflightError::Malformed(
                crate::BiffStructureError::EmptyStringContinuation,
            ))?;
        self.at += 1;
        Ok(b)
    }

    fn integer(&mut self, n: usize) -> Result<u32, PreflightError> {
        let mut value = 0;
        for i in 0..n {
            value |= (self.byte()? as u32) << (8 * i);
        }
        Ok(value)
    }

    fn skip(&mut self, mut n: usize) -> Result<(), PreflightError> {
        while n > 0 {
            if self.remaining() == 0 && !self.advance() {
                return Err(PreflightError::Malformed(
                    crate::BiffStructureError::TruncatedStringExtension,
                ));
            }
            let k = n.min(self.remaining());
            self.at += k;
            n -= k;
        }
        Ok(())
    }

    fn more(&self) -> bool {
        self.remaining() > 0 || self.part + 1 < self.parts.len()
    }
}

/// Inspect shared-string framing and resource claims without decoding the workbook.
pub(super) fn sst(
    rs: &[Record<'_>],
    budget: ResourceBudget,
    mut plan: SstAccounting,
) -> Result<SstAccounting, PreflightError> {
    let mut sizes = std::mem::take(&mut plan.sizes);
    sizes.clear();
    let first = rs
        .first()
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::MissingSstRecord,
        ))?
        .data;
    let unique = u32at(first, 4)? as usize;
    check(
        crate::BiffResource::SstUniqueStringClaim,
        unique as u64,
        budget.max_strings as u64,
    )?;
    check(
        crate::BiffResource::SstContinuationRecords,
        rs.len().saturating_sub(1) as u64,
        budget.max_continuations as u64,
    )?;
    let mut parts = vec![first.get(8..).ok_or(PreflightError::Malformed(
        crate::BiffStructureError::TruncatedSstCounts,
    ))?];
    parts.extend(rs.iter().skip(1).map(|r| r.data));
    let mut cursor = Segments {
        parts,
        part: 0,
        at: 0,
    };
    // Account for actual strings through the record chain, preserving the
    // historical tolerance for inaccurate SST counts.
    while cursor.more() {
        if cursor.remaining() == 0 {
            cursor.advance();
            if cursor.remaining() == 0 {
                continue;
            }
        }
        let chars = cursor.integer(2)? as usize;
        let flags = cursor.byte()?;
        check(
            crate::BiffResource::SharedStringCharacters,
            chars as u64,
            budget.max_string_chars as u64,
        )?;
        let runs = if flags & 8 != 0 {
            cursor.integer(2)? as usize
        } else {
            0
        };
        let ext = if flags & 4 != 0 {
            cursor.integer(4)? as usize
        } else {
            0
        };
        check(
            crate::BiffResource::StringExtensionBytes,
            ext as u64,
            budget.max_string_bytes as u64,
        )?;
        let mut wide = flags & 1 != 0;
        let mut left = chars;
        while left > 0 {
            if cursor.remaining() == 0 {
                if !cursor.advance() {
                    return Err(PreflightError::Malformed(
                        crate::BiffStructureError::TruncatedStringCharacters,
                    ));
                }
                wide = cursor.byte()? & 1 != 0;
            }
            let width = if wide { 2 } else { 1 };
            let take = left.min(cursor.remaining() / width);
            if take == 0 {
                return Err(PreflightError::Malformed(
                    crate::BiffStructureError::SplitUtf16CodeUnit,
                ));
            }
            cursor.skip(take * width)?;
            left -= take;
        }
        cursor.skip(runs * 4)?;
        cursor.skip(ext)?;
        check(
            crate::BiffResource::DecodedStrings,
            (plan.strings + 1) as u64,
            budget.max_strings as u64,
        )?;
        sizes.push(chars * 4);
        plan.strings += 1;
        plan.string_bytes =
            plan.string_bytes
                .checked_add(chars * 4)
                .ok_or(PreflightError::Representation(
                    crate::BiffRepresentationError::DecodedStringBytesOverflow,
                ))?;
        check(
            crate::BiffResource::DecodedStrings,
            plan.strings as u64,
            budget.max_strings as u64,
        )?;
        check(
            crate::BiffResource::DecodedStringBytes,
            plan.string_bytes as u64,
            budget.max_string_bytes as u64,
        )?;
    }
    plan.sizes = sizes;
    Ok(plan)
}
