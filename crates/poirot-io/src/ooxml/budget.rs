//! Measure generated XML and check package sizes before replacing ZIP entries.
use crate::IoError;
use std::io::Write;

/// A writer that counts generated XML bytes without storing them.
#[derive(Default)]
pub(super) struct SizeCounter(#[doc = "Current admitted cumulative byte count."] pub(super) u64);
impl Write for SizeCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self.0.checked_add(bytes.len() as u64).ok_or_else(|| {
            std::io::Error::other(crate::ExportLimitError::ExportPackageSizeOverflow)
        })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Replace an entry's contribution to the package total, checking both size limits.
pub(super) fn next_total(
    size: u64,
    previous: u64,
    total: u64,
    options: crate::WriteOptions,
) -> Result<u64, IoError> {
    let total = total
        .checked_sub(previous)
        .and_then(|n| n.checked_add(size))
        .ok_or(IoError::Limit(
            crate::ExportLimitError::ExportPackageSizeOverflow,
        ))?;
    if size > options.max_zip_entry_bytes || total > options.max_zip_total_bytes {
        return Err(IoError::Limit(crate::ExportLimitError::PackageBytes {
            entry_bytes: size,
            entry_limit: options.max_zip_entry_bytes,
            total_bytes: total,
            total_limit: options.max_zip_total_bytes,
        }));
    }
    Ok(total)
}
