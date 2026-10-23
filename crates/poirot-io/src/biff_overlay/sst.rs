//! Lossless SST/CONTINUE units from original BIFF records.
use crate::biff_preflight::{PreflightError, Record, ResourceBudget};
use poirot_model::ExcelText;
fn check(resource: &'static str, requested: u64, limit: u64) -> Result<(), PreflightError> {
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
fn u32at(data: &[u8], at: usize) -> Result<u32, PreflightError> {
    let bytes = data
        .get(
            at..at
                .checked_add(4)
                .ok_or(PreflightError::Malformed("integer offset overflow"))?,
        )
        .ok_or(PreflightError::Malformed("truncated SST counts"))?;
    // SAFETY: get above selects exactly four bytes or returns an error, so [u8; 4] conversion succeeds.
    Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
}
/// Cursor over SST/CONTINUE payloads preserving per-segment encoding switches.
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
            return Err(PreflightError::Malformed("truncated shared string"));
        }
        let b = *self
            .parts
            .get(self.part)
            .ok_or(PreflightError::Malformed("missing string continuation"))?
            .get(self.at)
            .ok_or(PreflightError::Malformed("empty string continuation"))?;
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
                return Err(PreflightError::Malformed("truncated string extension"));
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
/// Decode the shared-string table while preserving exact UTF-16 units.
pub(super) fn sst(
    rs: &[Record<'_>],
    budget: ResourceBudget,
) -> Result<Vec<ExcelText>, PreflightError> {
    let mut strings = Vec::new();
    let mut string_bytes = 0usize;
    let first = rs
        .first()
        .ok_or(PreflightError::Malformed("missing SST"))?
        .data;
    let unique = u32at(first, 4)? as usize;
    check(
        "SST unique string claim",
        unique as u64,
        budget.max_strings as u64,
    )?;
    check(
        "SST continuation records",
        rs.len().saturating_sub(1) as u64,
        budget.max_continuations as u64,
    )?;
    let mut parts = vec![
        first
            .get(8..)
            .ok_or(PreflightError::Malformed("truncated SST counts"))?,
    ];
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
            "shared string characters",
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
            "string extension bytes",
            ext as u64,
            budget.max_string_bytes as u64,
        )?;
        let mut wide = flags & 1 != 0;
        let mut left = chars;
        let mut units = Vec::with_capacity(chars);
        while left > 0 {
            if cursor.remaining() == 0 {
                if !cursor.advance() {
                    return Err(PreflightError::Malformed("truncated string characters"));
                }
                wide = cursor.byte()? & 1 != 0;
            }
            let width = if wide { 2 } else { 1 };
            let take = left.min(cursor.remaining() / width);
            if take == 0 {
                return Err(PreflightError::Malformed("split UTF16 code unit"));
            }
            for _ in 0..take {
                units.push(cursor.integer(width)? as u16);
            }
            left -= take;
        }
        cursor.skip(runs * 4)?;
        cursor.skip(ext)?;
        check(
            "decoded strings",
            (strings.len() + 1) as u64,
            budget.max_strings as u64,
        )?;
        strings.push(ExcelText::from_units(units));
        string_bytes =
            string_bytes
                .checked_add(chars * 4)
                .ok_or(PreflightError::Representation(
                    "decoded string bytes overflow",
                ))?;
        check(
            "decoded strings",
            strings.len() as u64,
            budget.max_strings as u64,
        )?;
        check(
            "decoded string bytes",
            string_bytes as u64,
            budget.max_string_bytes as u64,
        )?;
    }
    Ok(strings)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_sst_units_and_compression_switch_do_not_repair_surrogates() {
        let first = [3, 0, 0, 0, 3, 0, 0, 0, 2, 0, 1, 0x3d, 0xd8];
        let next = [1, 0, 0xde, 1, 0, 1, 0x3d, 0xd8, 1, 0, 0, 65];
        let rs = [
            Record {
                id: 0xfc,
                at: 0,
                data: &first,
            },
            Record {
                id: 0x3c,
                at: 18,
                data: &next,
            },
        ];
        let strings = sst(&rs, ResourceBudget::default()).unwrap();
        assert_eq!(strings[0].units(), &[0xd83d, 0xde00]);
        assert_eq!(strings[1].units(), &[0xd83d]);
        assert_eq!(strings[2].units(), &[65]);
        let budget = ResourceBudget {
            max_string_chars: 1,
            ..ResourceBudget::default()
        };
        assert!(matches!(
            sst(&rs, budget),
            Err(PreflightError::Resource {
                resource: "shared string characters",
                ..
            })
        ));
    }
}
