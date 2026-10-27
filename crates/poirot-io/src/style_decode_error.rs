/// Typed conditions reported by StyleDecode operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StyleDecodeError {
    /// Color requires one source.
    #[error("color requires one source")]
    ColorRequiresOneSource,
    /// Missing color.
    #[error("missing color")]
    MissingColor,
    /// Truncated style record.
    #[error("truncated style record")]
    TruncatedStyleRecord,
    /// DTD in style metadata.
    #[error("DTD in style metadata")]
    DtdInStyleMetadata,
    /// Metadata root/namespace.
    #[error("metadata root/namespace")]
    MetadataRootNamespace,
    /// XML depth.
    #[error("XML depth")]
    XmlDepth,
    /// DTD in metadata.
    #[error("DTD in metadata")]
    DtdInMetadata,
    /// Misplaced XML declaration.
    #[error("misplaced XML declaration")]
    MisplacedXmlDeclaration,
    /// CDATA outside metadata root.
    #[error("CDATA outside metadata root")]
    CdataOutsideMetadataRoot,
    /// Reference outside metadata root.
    #[error("reference outside metadata root")]
    ReferenceOutsideMetadataRoot,
    /// Unknown entity in metadata.
    #[error("unknown entity in metadata")]
    UnknownEntityInMetadata,
    /// Text outside metadata root.
    #[error("text outside metadata root")]
    TextOutsideMetadataRoot,
    /// Incomplete metadata root.
    #[error("incomplete metadata root")]
    IncompleteMetadataRoot,
    /// Missing font name.
    #[error("missing font name")]
    MissingFontName,
    /// Missing font size.
    #[error("missing font size")]
    MissingFontSize,
    /// Missing font scheme.
    #[error("missing font scheme")]
    MissingFontScheme,
    /// Missing font family.
    #[error("missing font family")]
    MissingFontFamily,
    /// Missing font charset.
    #[error("missing font charset")]
    MissingFontCharset,
    /// TextRotation.
    #[error("textRotation")]
    TextRotation,
    /// Dangling style component index.
    #[error("dangling style component index")]
    DanglingStyleComponentIndex,
    /// Missing numFmtId.
    #[error("missing numFmtId")]
    MissingNumfmtid,
    /// Missing formatCode.
    #[error("missing formatCode")]
    MissingFormatcode,
    /// Duplicate numFmtId.
    #[error("duplicate numFmtId")]
    DuplicateNumfmtid,
    /// Indexed palette exceeds 64.
    #[error("indexed palette exceeds 64")]
    IndexedPaletteExceeds64,
    /// Palette rgb.
    #[error("palette rgb")]
    PaletteRgb,
    /// Dangling custom numFmtId.
    #[error("dangling custom numFmtId")]
    DanglingCustomNumfmtid,
    /// Source token rejected by a style lexical codec.
    #[error("invalid SpreadsheetML style token {input:?}")]
    InvalidToken {
        /// Rejected source token.
        input: String,
    },
}
