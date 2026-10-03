//! Bounded CFB metadata, chain traversal, and Workbook/Book acquisition.
use std::collections::BTreeSet;

use super::{PreflightError, ResourceBudget, check, u16at, u32at};

const END: u32 = 0xffff_fffe;
fn sector(bytes: &[u8], size: usize, id: u32) -> Result<&[u8], PreflightError> {
    let start = (id as usize)
        .checked_add(1)
        .and_then(|x| x.checked_mul(size))
        .ok_or(PreflightError::Malformed("sector overflow"))?;
    let end = start
        .checked_add(size)
        .ok_or(PreflightError::Malformed("sector offset overflow"))?;
    bytes
        .get(start..end)
        .ok_or(PreflightError::Malformed("sector beyond input"))
}
fn chain(
    data: &[u8],
    size: usize,
    start: u32,
    fat: &[u32],
    len: Option<usize>,
    budget: ResourceBudget,
    regular: bool,
) -> Result<Vec<u8>, PreflightError> {
    if let Some(n) = len {
        check("CFB stream bytes", n as u64, budget.max_stream_bytes as u64)?;
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut id = start;
    while id != END {
        if seen.contains(&id) {
            return Err(PreflightError::Malformed("cyclic sector chain"));
        }
        check(
            "CFB chain sectors",
            (seen.len() + 1) as u64,
            budget.max_metadata_entries as u64,
        )?;
        seen.insert(id);
        let block = if regular {
            sector(data, size, id)?
        } else {
            let at = (id as usize)
                .checked_mul(size)
                .ok_or(PreflightError::Malformed("mini sector overflow"))?;
            let end = at
                .checked_add(size)
                .ok_or(PreflightError::Malformed("mini sector offset overflow"))?;
            data.get(at..end)
                .ok_or(PreflightError::Malformed("mini sector beyond root stream"))?
        };
        let take = len.map_or(size, |n| n.saturating_sub(out.len()).min(size));
        check(
            "CFB stream bytes",
            (out.len() + take) as u64,
            budget.max_stream_bytes as u64,
        )?;
        out.extend_from_slice(
            block
                .get(..take)
                .ok_or(PreflightError::Malformed("truncated stream block"))?,
        );
        // Validate the entire chain even when a declared length is shorter.
        id = *fat
            .get(id as usize)
            .ok_or(PreflightError::Malformed("sector has no FAT entry"))?;
    }
    if len.is_some_and(|n| out.len() != n) {
        return Err(PreflightError::Malformed(
            "stream chain shorter than declared length",
        ));
    }
    Ok(out)
}
/// Validate compound-file stream metadata and extract admitted workbook bytes.
pub(super) fn workbook_stream(
    bytes: &[u8],
    budget: ResourceBudget,
) -> Result<Vec<u8>, PreflightError> {
    if bytes.get(..8) != Some(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        return Err(PreflightError::Malformed("CFB signature"));
    }
    let shift = u16at(bytes, 30)?;
    if !matches!(shift, 9 | 12) || u16at(bytes, 32)? != 6 {
        return Err(PreflightError::Malformed("sector size"));
    }
    let size = 1usize << shift;
    if bytes.len() < size {
        return Err(PreflightError::Malformed("CFB header"));
    }
    for (at, label) in [
        (44, "CFB FAT capacity"),
        (64, "CFB mini FAT sectors"),
        (72, "CFB DIFAT sectors"),
        (62, "backend DIFAT capacity"),
    ] {
        check(
            label,
            u32at(bytes, at)? as u64,
            budget.max_metadata_entries as u64,
        )?;
    }
    let mut difat = Vec::new();
    // SAFETY: The step is a nonzero integer literal.
    for at in (76..512).step_by(4) {
        let id = u32at(bytes, at)?;
        if id < 0xffff_fffc {
            check(
                "FAT sectors",
                (difat.len() + 1) as u64,
                budget.max_metadata_entries as u64,
            )?;
            difat.push(id);
        }
    }
    let mut id = u32at(bytes, 68)?;
    let mut seen = BTreeSet::new();
    while id < 0xffff_fffa {
        if seen.contains(&id) {
            return Err(PreflightError::Malformed("cyclic DIFAT"));
        }
        check(
            "DIFAT sectors",
            (seen.len() + 1) as u64,
            budget.max_metadata_entries as u64,
        )?;
        seen.insert(id);
        let s = sector(bytes, size, id)?;
        // SAFETY: The step is a nonzero integer literal.
        for at in (0..size - 4).step_by(4) {
            let next = u32at(s, at)?;
            if next < 0xffff_fffc {
                check(
                    "FAT sectors",
                    (difat.len() + 1) as u64,
                    budget.max_metadata_entries as u64,
                )?;
                difat.push(next);
            }
        }
        id = u32at(s, size - 4)?;
    }
    let mut fat = Vec::new();
    for id in difat {
        let s = sector(bytes, size, id)?;
        check(
            "FAT entries",
            (fat.len() + size / 4) as u64,
            budget.max_metadata_entries as u64,
        )?;
        // SAFETY: The step is a nonzero integer literal.
        for at in (0..size).step_by(4) {
            fat.push(u32at(s, at)?);
        }
    }
    let dirs = chain(bytes, size, u32at(bytes, 48)?, &fat, None, budget, true)?;
    let root = dirs
        .get(..128)
        .ok_or(PreflightError::Malformed("missing root directory"))?;
    let length = |d: &[u8]| -> Result<usize, PreflightError> {
        let low = u32at(d, 120)? as u64;
        let high = if shift == 12 {
            u32at(d, 124)? as u64
        } else {
            0
        };
        usize::try_from(low | (high << 32))
            .map_err(|_| PreflightError::Representation("stream length exceeds address space"))
    };
    let (mini, mini_fat) = if u32at(bytes, 64)? != 0 {
        let mini = chain(
            bytes,
            size,
            u32at(root, 116)?,
            &fat,
            Some(length(root)?),
            budget,
            true,
        )?;
        let mf = chain(
            bytes,
            size,
            u32at(bytes, 60)?,
            &fat,
            Some(
                (u32at(bytes, 64)? as usize)
                    .checked_mul(size)
                    .ok_or(PreflightError::Malformed("mini FAT size overflow"))?,
            ),
            budget,
            true,
        )?;
        let mut table = Vec::new();
        check(
            "mini FAT entries",
            (mf.len() / 4) as u64,
            budget.max_metadata_entries as u64,
        )?;
        // SAFETY: The step is a nonzero integer literal.
        for at in (0..mf.len()).step_by(4) {
            table.push(u32at(&mf, at)?);
        }
        (mini, table)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut workbook = None;
    // SAFETY: The chunk size is the nonzero literal 128.
    for d in dirs.as_chunks::<128>().0 {
        let name: Vec<u16> = (0..64)
            // SAFETY: The step is a nonzero integer literal.
            .step_by(2)
            // SAFETY: as_chunks::<128>() supplies a 128-byte directory; at ranges over 0,2,..62, so each two-byte read fits.
            .map(|at| u16at(d, at).unwrap())
            .take_while(|c| *c != 0)
            .collect();
        if name == "Workbook".encode_utf16().collect::<Vec<_>>()
            || name == "Book".encode_utf16().collect::<Vec<_>>()
        {
            let n = length(d)?;
            let stream = if n < 4096 {
                chain(&mini, 64, u32at(d, 116)?, &mini_fat, Some(n), budget, false)?
            } else {
                chain(bytes, size, u32at(d, 116)?, &fat, Some(n), budget, true)?
            };
            workbook = Some(stream);
            break;
        }
    }
    workbook.ok_or(PreflightError::Malformed("missing Workbook/Book stream"))
}
