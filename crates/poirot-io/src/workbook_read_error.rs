/// Typed conditions reported by WorkbookRead operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkbookReadError {
    /// Missing text worksheet.
    #[error("missing text worksheet")]
    MissingTextWorksheet,
    /// Unsupported cell content after adaptation.
    #[error("unsupported cell content after adaptation")]
    UnsupportedCellContentAfterAdaptation,
    /// Backend worksheet inventory differs from checked OPC plan.
    #[error("backend worksheet inventory differs from checked OPC plan")]
    BackendWorksheetInventoryDiffersFromCheckedOpcPlan,
    /// Shared formula has no master.
    #[error("shared formula has no master")]
    SharedFormulaHasNoMaster,
    /// Missing shared follower.
    #[error("missing shared follower")]
    MissingSharedFollower,
    /// Generated package has no styles part.
    #[error("generated package has no styles part")]
    GeneratedPackageHasNoStylesPart,
    /// Overlapping array formula cache ranges.
    #[error("overlapping array formula cache ranges")]
    OverlappingArrayFormulaCacheRanges,
    /// Missing array worksheet.
    #[error("missing array worksheet")]
    MissingArrayWorksheet,
    /// Zero row index.
    #[error("zero row index")]
    ZeroRowIndex,
    /// Row overflow.
    #[error("row overflow")]
    RowOverflow,
    /// Column overflow.
    #[error("column overflow")]
    ColumnOverflow,
    /// Invalid array formula cache range.
    #[error("invalid array formula cache range")]
    InvalidArrayFormulaCacheRange,
    /// Missing modern error cell.
    #[error("missing modern error cell")]
    MissingModernErrorCell,
    /// Array cache has no formula anchor.
    #[error("array cache has no formula anchor")]
    ArrayCacheHasNoFormulaAnchor,
    /// Array cache overlaps an authored formula.
    #[error("array cache overlaps an authored formula")]
    ArrayCacheOverlapsAnAuthoredFormula,
    /// Shared string index out of range.
    #[error("shared string index out of range")]
    SharedStringIndexOutOfRange,
    /// Backend omitted a stored cell value.
    #[error("backend omitted a stored cell value")]
    BackendOmittedAStoredCellValue,
    /// Cell style index out of range.
    #[error("cell style index out of range")]
    CellStyleIndexOutOfRange,
    /// Invalid worksheet visibility.
    #[error("invalid worksheet visibility")]
    InvalidWorksheetVisibility,
    /// Defined name without name.
    #[error("defined name without name")]
    DefinedNameWithoutName,
    /// Unregistered export style.
    #[error("unregistered export style")]
    UnregisteredExportStyle,
}
