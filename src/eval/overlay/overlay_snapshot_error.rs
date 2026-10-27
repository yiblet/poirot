/// Semantic rejection of persisted overlay state.
#[derive(Debug, thiserror::Error)]
pub enum OverlaySnapshotError {
    /// Baseline differs from detached state.
    #[error("baseline differs from detached state")]
    BaselineDiffersFromDetachedState,
    /// Cached results lack execution context.
    #[error("cached results lack execution context")]
    CachedResultsLackExecutionContext,
    /// Duplicate cached cell.
    #[error("duplicate cached cell")]
    DuplicateCachedCell,
    /// Duplicate overlay write.
    #[error("duplicate overlay write")]
    DuplicateOverlayWrite,
    /// Edited checkpoint lacks its baseline.
    #[error("edited checkpoint lacks its baseline")]
    EditedCheckpointLacksItsBaseline,
    /// Incomplete cached spill footprint.
    #[error("incomplete cached spill footprint")]
    IncompleteCachedSpillFootprint,
    /// Invalid cached spill ownership.
    #[error("invalid cached spill ownership")]
    InvalidCachedSpillOwnership,
    /// Missing cached spill owner.
    #[error("missing cached spill owner")]
    MissingCachedSpillOwner,
    /// Noncanonical source discovery prefixes.
    #[error("noncanonical source discovery prefixes")]
    NoncanonicalSourceDiscoveryPrefixes,
    /// Parser policy mismatch.
    #[error("parser policy mismatch")]
    ParserPolicyMismatch,
    /// Snapshot baseline belongs to a checkpoint source section.
    #[error("snapshot baseline belongs to a checkpoint source section")]
    SnapshotBaselineBelongsToACheckpointSourceSection,
    /// Snapshot ownership conversion failed.
    #[error("snapshot ownership conversion failed")]
    SnapshotOwnershipConversionFailed,
    /// Source and prepared state disagree.
    #[error("source and prepared state disagree")]
    SourceAndPreparedStateDisagree,
    /// Spill child lacks its producer.
    #[error("spill child lacks its producer")]
    SpillChildLacksItsProducer,
    /// Spill producer lacks footprint bounds.
    #[error("spill producer lacks footprint bounds")]
    SpillProducerLacksFootprintBounds,
    /// Spill producer lacks source formula.
    #[error("spill producer lacks source formula")]
    SpillProducerLacksSourceFormula,
    /// Style-only marker changes source content.
    #[error("style-only marker changes source content")]
    StyleOnlyMarkerChangesSourceContent,
    /// Style-only marker lacks an override.
    #[error("style-only marker lacks an override")]
    StyleOnlyMarkerLacksAnOverride,
    /// Unsupported snapshot version.
    #[error("unsupported snapshot version")]
    UnsupportedSnapshotVersion,
    /// Write and removal overlap.
    #[error("write and removal overlap")]
    WriteAndRemovalOverlap,
    /// Rejection from the borrow subsystem.
    #[error("could not restore borrow state")]
    Borrow(#[from] std::cell::BorrowError),
    /// Rejection from the codec subsystem.
    #[error("could not restore codec state")]
    Codec(#[from] postcard::Error),
    /// Rejection from the plan subsystem.
    #[error("could not restore plan state")]
    Plan(#[from] crate::eval::compile::compiler::PlanSnapshotError),
    /// Rejection from the lookupprefix subsystem.
    #[error("could not restore lookupprefix state")]
    LookupPrefix(#[from] crate::eval::reuse::LookupPrefixError),
    /// Restored result cache accounting is invalid.
    #[error("restored result cache rejected")]
    Cache(#[from] crate::eval::reuse::CacheError),
}
