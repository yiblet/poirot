/// Semantic rejection of persisted artifact state.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotEnvelopeError {
    /// Compatible prepared state required but absent.
    #[error("compatible prepared state required but absent")]
    CompatiblePreparedStateRequiredButAbsent,
    /// File byte limit.
    #[error("file byte limit")]
    FileByteLimit,
    /// Header offset overflow.
    #[error("header offset overflow")]
    HeaderOffsetOverflow,
    /// Header word width.
    #[error("header word width")]
    HeaderWordWidth,
    /// Incompatible prepared engine revision.
    #[error("incompatible prepared engine revision")]
    IncompatiblePreparedEngineRevision,
    /// Invalid magic or truncated header.
    #[error("invalid magic or truncated header")]
    InvalidMagicOrTruncatedHeader,
    /// Kind does not match prepared section.
    #[error("kind does not match prepared section")]
    KindDoesNotMatchPreparedSection,
    /// Prepared length overflow.
    #[error("prepared length overflow")]
    PreparedLengthOverflow,
    /// Section lengths or trailing data.
    #[error("section lengths or trailing data")]
    SectionLengthsOrTrailingData,
    /// Source length overflow.
    #[error("source length overflow")]
    SourceLengthOverflow,
    /// Source schema missing.
    #[error("source schema missing")]
    SourceSchemaMissing,
    /// Trailing section bytes.
    #[error("trailing section bytes")]
    TrailingSectionBytes,
    /// Truncated header word.
    #[error("truncated header word")]
    TruncatedHeaderWord,
    /// Truncated prepared section.
    #[error("truncated prepared section")]
    TruncatedPreparedSection,
    /// Unknown kind or header flags.
    #[error("unknown kind or header flags")]
    UnknownKindOrHeaderFlags,
    /// Unsupported file version.
    #[error("unsupported file version")]
    UnsupportedFileVersion,
    /// Unsupported source schema.
    #[error("unsupported source schema")]
    UnsupportedSourceSchema,
    /// Checksum disagrees with prepared source identity.
    #[error("source and prepared state disagree")]
    SourceAndPreparedStateDisagree,
}
