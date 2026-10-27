/// Typed conditions reported by ExportLimit operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportLimitError {
    /// Export source size overflow.
    #[error("export source size overflow")]
    ExportSourceSizeOverflow,
    /// Style context size overflow.
    #[error("style context size overflow")]
    StyleContextSizeOverflow,
    /// Export row count overflow.
    #[error("export row count overflow")]
    ExportRowCountOverflow,
    /// Export cell count overflow.
    #[error("export cell count overflow")]
    ExportCellCountOverflow,
    /// UTF16 source size overflow.
    #[error("UTF16 source size overflow")]
    Utf16SourceSizeOverflow,
    /// Export worksheet payload overflow.
    #[error("export worksheet payload overflow")]
    ExportWorksheetPayloadOverflow,
    /// Style size overflow.
    #[error("style size overflow")]
    StyleSizeOverflow,
    /// Export package size overflow.
    #[error("export package size overflow")]
    ExportPackageSizeOverflow,
    /// Style index overflow.
    #[error("style index overflow")]
    StyleIndexOverflow,
    /// Number-format ID overflow.
    #[error("number-format ID overflow")]
    NumberFormatIdOverflow,
    /// Source payload exceeds configured byte ceiling.
    #[error("export source payload {requested} exceeds limit {limit}")]
    SourceBytes {
        /// Actual source bytes.
        requested: u64,
        /// Configured ceiling.
        limit: u64,
    },
    /// Worksheet text exceeds configured entry ceiling.
    #[error("worksheet {sheet:?} text payload {requested} exceeds limit {limit}")]
    WorksheetTextBytes {
        /// Worksheet name.
        sheet: String,
        /// Actual text bytes.
        requested: u64,
        /// Configured ceiling.
        limit: u64,
    },
    /// Export package entry bytes exceed a configured ceiling.
    #[error("export entry bytes {entry_bytes} or total {total_bytes} exceeds configured ceilings")]
    PackageBytes {
        /// Current entry size.
        entry_bytes: u64,
        /// Configured entry ceiling.
        entry_limit: u64,
        /// Cumulative size.
        total_bytes: u64,
        /// Configured total ceiling.
        total_limit: u64,
    },
    /// Compressed output exceeds configured byte ceiling.
    #[error("export ZIP bytes {requested} exceeds limit {limit}")]
    OutputBytes {
        /// Actual package bytes.
        requested: u64,
        /// Configured ceiling.
        limit: u64,
    },
    /// Generated package exceeds import admission ceilings.
    #[error("generated package exceeds quota")]
    GeneratedQuota(#[source] crate::IoQuotaError),
    /// Export package entry count exceeds the configured ceiling.
    #[error("export package entries {requested} exceeds limit {limit}")]
    EntryCount {
        /// Actual entry count.
        requested: usize,
        /// Configured ceiling.
        limit: usize,
    },
}
