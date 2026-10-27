//! One framed BIFF record.

/// Borrowed checked BIFF record header location and payload.
#[derive(Clone, Copy)]
pub(in crate::xls) struct Record<'a> {
    /// BIFF record type identifier.
    pub(in crate::xls) id: u16,
    /// Record byte offset within the workbook stream.
    pub(in crate::xls) at: usize,
    /// Borrowed payload excluding the BIFF record header.
    pub(in crate::xls) data: &'a [u8],
}
