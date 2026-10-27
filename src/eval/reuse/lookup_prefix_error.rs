/// Structural rejection of persisted lookup search evidence.
#[derive(Debug, thiserror::Error)]
pub enum LookupPrefixError {
    /// Lookup prefix axis geometry.
    #[error("lookup prefix axis geometry")]
    LookupPrefixAxisGeometry,
    /// Lookup prefix selection or numeric key.
    #[error("lookup prefix selection or numeric key")]
    LookupPrefixSelectionOrNumericKey,
    /// Lookup prefix storage limit.
    #[error("lookup prefix storage limit")]
    LookupPrefixStorageLimit,
    /// Lookup prefix text storage overflow.
    #[error("lookup prefix text storage overflow")]
    LookupPrefixTextStorageOverflow,
    /// Lookup prefix worksheet or empty partition.
    #[error("lookup prefix worksheet or empty partition")]
    LookupPrefixWorksheetOrEmptyPartition,
    /// Persisted coordinate lies outside workbook bounds.
    #[error("persisted coordinate rejected")]
    Coordinate(#[from] poirot_io::ModelError),
}
