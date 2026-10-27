//! Bounded CFB metadata, chain traversal, Workbook/Book acquisition, and encrypted-XLSX detection.
use std::collections::BTreeSet;

use super::checks::{check, u16at, u32at};
use super::preflight_error::PreflightError;
use super::resource_budget::ResourceBudget;

const END: u32 = 0xffff_fffe;

fn sector(bytes: &[u8], size: usize, id: u32) -> Result<&[u8], PreflightError> {
    let start = (id as usize)
        .checked_add(1)
        .and_then(|x| x.checked_mul(size))
        .ok_or(PreflightError::Malformed(
            crate::BiffStructureError::SectorOverflow,
        ))?;
    let end = start.checked_add(size).ok_or(PreflightError::Malformed(
        crate::BiffStructureError::SectorOffsetOverflow,
    ))?;
    bytes.get(start..end).ok_or(PreflightError::Malformed(
        crate::BiffStructureError::SectorBeyondInput,
    ))
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
        check(
            crate::BiffResource::CfbStreamBytes,
            n as u64,
            budget.max_stream_bytes as u64,
        )?;
    }
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut id = start;
    while id != END {
        if seen.contains(&id) {
            return Err(PreflightError::Malformed(
                crate::BiffStructureError::CyclicSectorChain,
            ));
        }
        check(
            crate::BiffResource::CfbChainSectors,
            (seen.len() + 1) as u64,
            budget.max_metadata_entries as u64,
        )?;
        seen.insert(id);
        let block = if regular {
            sector(data, size, id)?
        } else {
            let at = (id as usize)
                .checked_mul(size)
                .ok_or(PreflightError::Malformed(
                    crate::BiffStructureError::MiniSectorOverflow,
                ))?;
            let end = at.checked_add(size).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::MiniSectorOffsetOverflow,
            ))?;
            data.get(at..end).ok_or(PreflightError::Malformed(
                crate::BiffStructureError::MiniSectorBeyondRootStream,
            ))?
        };
        let take = len.map_or(size, |n| n.saturating_sub(out.len()).min(size));
        check(
            crate::BiffResource::CfbStreamBytes,
            (out.len() + take) as u64,
            budget.max_stream_bytes as u64,
        )?;
        out.extend_from_slice(block.get(..take).ok_or(PreflightError::Malformed(
            crate::BiffStructureError::TruncatedStreamBlock,
        ))?);
        // Validate the entire chain even when a declared length is shorter.
        id = *fat.get(id as usize).ok_or(PreflightError::Malformed(
            crate::BiffStructureError::SectorHasNoFatEntry,
        ))?;
    }
    if len.is_some_and(|n| out.len() != n) {
        return Err(PreflightError::Malformed(
            crate::BiffStructureError::StreamChainShorterThanDeclaredLength,
        ));
    }
    Ok(out)
}

/// Validate compound-file stream metadata and extract admitted workbook bytes.
pub(in crate::xls) fn workbook_stream(
    bytes: &[u8],
    budget: ResourceBudget,
) -> Result<Vec<u8>, PreflightError> {
    if bytes.get(..8) != Some(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        return Err(PreflightError::Malformed(
            crate::BiffStructureError::CfbSignature,
        ));
    }
    let shift = u16at(bytes, 30)?;
    if !matches!(shift, 9 | 12) || u16at(bytes, 32)? != 6 {
        return Err(PreflightError::Malformed(
            crate::BiffStructureError::SectorSize,
        ));
    }
    let size = 1usize << shift;
    if bytes.len() < size {
        return Err(PreflightError::Malformed(
            crate::BiffStructureError::CfbHeader,
        ));
    }
    for (at, label) in [
        (44, crate::BiffResource::CfbFatCapacity),
        (64, crate::BiffResource::CfbMiniFatSectors),
        (72, crate::BiffResource::CfbDifatSectors),
        (62, crate::BiffResource::BackendDifatCapacity),
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
                crate::BiffResource::FatSectors,
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
            return Err(PreflightError::Malformed(
                crate::BiffStructureError::CyclicDifat,
            ));
        }
        check(
            crate::BiffResource::DifatSectors,
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
                    crate::BiffResource::FatSectors,
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
            crate::BiffResource::FatEntries,
            (fat.len() + size / 4) as u64,
            budget.max_metadata_entries as u64,
        )?;
        // SAFETY: The step is a nonzero integer literal.
        for at in (0..size).step_by(4) {
            fat.push(u32at(s, at)?);
        }
    }
    let dirs = chain(bytes, size, u32at(bytes, 48)?, &fat, None, budget, true)?;
    let root = dirs.get(..128).ok_or(PreflightError::Malformed(
        crate::BiffStructureError::MissingRootDirectory,
    ))?;
    let length = |d: &[u8]| -> Result<usize, PreflightError> {
        let low = u32at(d, 120)? as u64;
        let high = if shift == 12 {
            u32at(d, 124)? as u64
        } else {
            0
        };
        usize::try_from(low | (high << 32)).map_err(|_| {
            PreflightError::Representation(
                crate::BiffRepresentationError::StreamLengthExceedsAddressSpace,
            )
        })
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
            Some((u32at(bytes, 64)? as usize).checked_mul(size).ok_or(
                PreflightError::Malformed(crate::BiffStructureError::MiniFatSizeOverflow),
            )?),
            budget,
            true,
        )?;
        let mut table = Vec::new();
        check(
            crate::BiffResource::MiniFatEntries,
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
    let utf16 = |s: &str| s.encode_utf16().collect::<Vec<_>>();
    let (workbook_name, book_name) = (utf16("Workbook"), utf16("Book"));
    let (info_name, package_name) = (utf16("EncryptionInfo"), utf16("EncryptedPackage"));
    let (mut info, mut package) = (false, false);
    // SAFETY: The chunk size is the nonzero literal 128.
    for d in dirs.as_chunks::<128>().0 {
        let name: Vec<u16> = (0..64)
            // SAFETY: The step is a nonzero integer literal.
            .step_by(2)
            // SAFETY: as_chunks::<128>() supplies a 128-byte directory; at ranges over 0,2,..62, so each two-byte read fits.
            .map(|at| u16at(d, at).unwrap())
            .take_while(|c| *c != 0)
            .collect();
        if name == workbook_name || name == book_name {
            let n = length(d)?;
            return if n < 4096 {
                chain(&mini, 64, u32at(d, 116)?, &mini_fat, Some(n), budget, false)
            } else {
                chain(bytes, size, u32at(d, 116)?, &fat, Some(n), budget, true)
            };
        }
        // Object type 2 is a stream (MS-CFB 2.6.1); free entries may keep stale names.
        if d.get(66) == Some(&2) {
            info |= name == info_name;
            package |= name == package_name;
        }
    }
    // MS-OFFCRYPTO 2.3.4 (ECMA-376 document encryption) stores encrypted OOXML in these two streams.
    Err(if info && package {
        PreflightError::EncryptedXlsx
    } else {
        PreflightError::Malformed(crate::BiffStructureError::MissingWorkbookBookStream)
    })
}
