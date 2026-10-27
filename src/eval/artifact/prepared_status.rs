/// Outcome for the optional prepared section; source is always restored and validated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreparedStatus {
    /// The file contained only source.
    Absent,
    /// Prepared state was validated; providers still need normal registry attachment.
    Restored,
    /// Source and authored edits recovered; engine-prepared state and policies were incompatible.
    DiscardedIncompatible,
    /// Source and authored edits recovered as explicitly requested.
    DiscardedByRequest,
}
