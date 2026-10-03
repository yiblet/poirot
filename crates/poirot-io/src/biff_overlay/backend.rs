//! Same-length patches applied only to a cloned backend view.
use super::{PreflightError, Result};
use std::io::{Cursor, Write};

/// Targeted BIFF rewrite making unsupported source records safe for backend decoding.
pub(super) enum BackendPatch {
    /// Hide a source name record that the fidelity overlay restores separately.
    SuppressName {
        /// Byte offset of the source BIFF record to suppress.
        at: usize,
    },
    /// Substitute a backend-safe formula while retaining original source separately.
    FormulaPlaceholder {
        /// Byte offset of the formula record to patch.
        at: usize,
    },
}

/// Apply admitted record patches to bytes used only for backend decoding.
pub(super) fn backend_view(
    bytes: &[u8],
    stream: &[u8],
    patches: &[BackendPatch],
) -> Result<Vec<u8>> {
    let mut view = stream.to_vec();
    for patch in patches {
        match *patch {
            BackendPatch::SuppressName { at } => {
                // SAFETY: The checked destination range and the byte array each contain 2 bytes.
                view.get_mut(
                    at..at
                        .checked_add(2)
                        .ok_or(PreflightError::Malformed("patch offset overflow"))?,
                )
                .ok_or(PreflightError::Malformed("patch beyond stream"))?
                .copy_from_slice(&0xffffu16.to_le_bytes());
            }
            BackendPatch::FormulaPlaceholder { at } => {
                // SAFETY: The checked destination range and the byte array each contain 2 bytes.
                view.get_mut(
                    at.checked_add(24)
                        .ok_or(PreflightError::Malformed("patch offset overflow"))?
                        ..at.checked_add(26)
                            .ok_or(PreflightError::Malformed("patch offset overflow"))?,
                )
                .ok_or(PreflightError::Malformed("patch beyond stream"))?
                .copy_from_slice(&3u16.to_le_bytes());
                // SAFETY: The checked destination range and the byte array each contain 3 bytes.
                view.get_mut(
                    at.checked_add(26)
                        .ok_or(PreflightError::Malformed("patch offset overflow"))?
                        ..at.checked_add(29)
                            .ok_or(PreflightError::Malformed("patch offset overflow"))?,
                )
                .ok_or(PreflightError::Malformed("patch beyond stream"))?
                .copy_from_slice(&[0x1e, 0, 0]);
            }
        }
    }
    let mut compound = cfb::CompoundFile::open(Cursor::new(bytes.to_vec()))
        .map_err(|_| PreflightError::Malformed("backend compound view"))?;
    let path = if compound.exists("/Workbook") {
        "/Workbook"
    } else {
        "/Book"
    };
    {
        let mut file = compound
            .open_stream(path)
            .map_err(|_| PreflightError::Malformed("backend Workbook view"))?;
        file.write_all(&view)
            .map_err(|_| PreflightError::Malformed("backend view write"))?;
    }
    Ok(compound.into_inner().into_inner())
}
