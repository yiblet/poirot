/// Typed conditions reported by WorkbookFeature operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkbookFeatureError {
    /// Input is neither an XLSX ZIP nor an XLS compound file.
    #[error("input is neither an XLSX ZIP nor an XLS compound file")]
    UnsupportedFileSignature,
    /// Export formula spellings do not match sheets.
    #[error("export formula spellings do not match sheets")]
    FormulaSheetInventory,
    /// Cannot export workbook without worksheets.
    #[error("cannot export workbook without worksheets")]
    MissingWorksheets,
    /// Invalid text representation.
    #[error("invalid text representation")]
    InvalidTextRepresentation,
    /// Cannot export workbook without a visible worksheet.
    #[error("cannot export workbook without a visible worksheet")]
    MissingVisibleWorksheet,
    /// Column exceeds XLSX grid.
    #[error("column exceeds XLSX grid")]
    ColumnExceedsXlsxGrid,
    /// Spill formula export size overflow.
    #[error("spill formula export size overflow")]
    SpillFormulaExportSizeOverflow,
    /// Invalid spill cache anchor geometry.
    #[error("invalid spill cache anchor geometry")]
    InvalidSpillCacheAnchorGeometry,
    /// Spill child lies outside declared extent.
    #[error("spill child lies outside declared extent")]
    SpillChildLiesOutsideDeclaredExtent,
    /// ISO date has no exact typed model representation.
    #[error("ISO date has no exact typed model representation")]
    IsoDate,
    /// Nonfinite formula cache.
    #[error("nonfinite formula cache")]
    NonfiniteFormulaCache,
    /// Row index overflow.
    #[error("row index overflow")]
    RowIndexOverflow,
    /// Style string contains invalid XML character.
    #[error("style string contains invalid XML character")]
    StyleStringContainsInvalidXmlCharacter,
    /// Source package repeats an entry identity.
    #[error("duplicate ZIP entry {name:?}")]
    DuplicateZipEntry {
        /// Conflicting entry path.
        name: String,
    },
    /// Backend ISO date or duration has no lossless model representation.
    #[error("ISO date or duration {input:?} has no model representation")]
    IsoDateOrDuration {
        /// Backend source text.
        input: String,
    },
    /// Spill cache would overwrite authored content.
    #[error("spill cache rectangle contains authored content at {sheet}!{address}")]
    AuthoredSpillContent {
        /// Worksheet name.
        sheet: String,
        /// Authored cell coordinate.
        address: crate::CellAddress,
    },
}
