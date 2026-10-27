//! Ordered workbook globals and cumulative shared-string accounting.
use super::checks::{u16at, u32at};
use super::preflight_error::PreflightError;
use super::record::Record;
use super::resource_budget::ResourceBudget;
use super::sst_accounting::{SstAccounting, sst};

/// Checked sheet positions and shared-string accounting recovered from BIFF globals.
pub(super) struct GlobalsPlan {
    /// Worksheet substream offsets declared by workbook globals.
    pub(super) positions: Vec<usize>,
    /// Resource counts carried into worksheet preflight.
    pub(super) accounting: SstAccounting,
}

/// Inspect workbook globals and admit their declared resource claims.
pub(super) fn globals(
    rs: &[Record<'_>],
    budget: ResourceBudget,
) -> Result<GlobalsPlan, PreflightError> {
    // FILEPASS (MS-XLS 2.4.117) encrypts every later record payload, including
    // the SST, so refuse it before any payload is parsed.
    if rs.iter().take_while(|r| r.id != 0xa).any(|r| r.id == 0x2f) {
        return Err(PreflightError::Representation(
            "encrypted XLS workbooks are not supported",
        ));
    }
    let mut positions = Vec::new();
    let mut accounting = SstAccounting::default();
    let mut i = 0;
    let mut biff8 = true;
    while i < rs.len() {
        // SAFETY: Record cursors are bounded by rs.len(); minimum-length checks admit r.data[3], and cce <= data.len() is checked before taking its suffix.
        let r = rs[i];
        if r.id == 0xa {
            break;
        }
        if r.id == 0x809 && r.data.len() >= 2 {
            biff8 = u16at(r.data, 0)? == 0x600;
        }
        let minimum = match r.id {
            0x22 | 0x42 | 0x17 | 0x2f | 0x809 => 2,
            0x85 => 7,
            0x18 => 14,
            0xe0 => 4,
            0x41e => 4,
            0xe5 => 2,
            _ => 0,
        };
        if r.data.len() < minimum {
            return Err(PreflightError::Malformed("truncated metadata record"));
        }
        if r.id == 0x18 {
            // SAFETY: Record cursors are bounded by rs.len(); minimum-length checks admit r.data[3], and cce <= data.len() is checked before taking its suffix.
            let cch = r.data[3] as usize;
            let cce = u16at(r.data, 4)? as usize;
            if cce > r.data.len() || r.data.len() < 14 + cch + usize::from(biff8) {
                return Err(PreflightError::Malformed("name token/string length"));
            }
            // SAFETY: Record cursors are bounded by rs.len(); minimum-length checks admit r.data[3], and cce <= data.len() is checked before taking its suffix.
            let token = &r.data[r.data.len() - cce..];
            if let Some(id) = token.first() {
                let need = match id {
                    0x3a | 0x5a | 0x7a => {
                        if biff8 {
                            7
                        } else {
                            18
                        }
                    }
                    0x3b | 0x5b | 0x7b => {
                        if biff8 {
                            11
                        } else {
                            21
                        }
                    }
                    0x3c | 0x5c | 0x7c | 0x3d | 0x5d | 0x7d => {
                        if biff8 {
                            3
                        } else {
                            13
                        }
                    }
                    _ => 0,
                };
                if token.len() < need {
                    return Err(PreflightError::Malformed(
                        "truncated defined-name reference token",
                    ));
                }
            }
        }

        if r.id == 0x85 {
            let pos = u32at(r.data, 0)? as usize;
            if rs.binary_search_by_key(&pos, |r| r.at).is_err() {
                return Err(PreflightError::Malformed(
                    "sheet offset is not a record boundary",
                ));
            }
            positions.push(pos);
        }
        if r.id == 0xfc {
            let mut end = i + 1;
            // SAFETY: Record cursors are bounded by rs.len(); minimum-length checks admit r.data[3], and cce <= data.len() is checked before taking its suffix.
            while end < rs.len() && rs[end].id == 0x3c {
                end += 1;
            }
            // SAFETY: Record cursors are bounded by rs.len(); minimum-length checks admit r.data[3], and cce <= data.len() is checked before taking its suffix.
            accounting = sst(&rs[i..end], budget, accounting)?;
            i = end;
            continue;
        }
        i += 1;
    }
    Ok(GlobalsPlan {
        positions,
        accounting,
    })
}
