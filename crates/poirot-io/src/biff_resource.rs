/// BIFF admission resource measured by its corresponding budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiffResource {
    /// import identifier comparisons.
    ImportIdentifierComparisons,
    /// shared string characters.
    SharedStringCharacters,
    /// cached string characters.
    CachedStringCharacters,
    /// cached STRING continuations.
    CachedStringContinuations,
    /// XTI declarations.
    XtiDeclarations,
    /// LABEL string characters.
    LabelStringCharacters,
    /// BIFF scan work.
    BiffScanWork,
    /// decoded cells.
    DecodedCells,
    /// add-in name characters.
    AddInNameCharacters,
    /// add-in declaration bytes.
    AddInDeclarationBytes,
    /// BIFF formula tokens.
    BiffFormulaTokens,
    /// BIFF formula bytes.
    BiffFormulaBytes,
    /// SST continuation records.
    SstContinuationRecords,
    /// CFB stream bytes.
    CfbStreamBytes,
    /// cell text bytes.
    CellTextBytes,
    /// SST unique string claim.
    SstUniqueStringClaim,
    /// string extension bytes.
    StringExtensionBytes,
    /// decoded strings.
    DecodedStrings,
    /// decoded string bytes.
    DecodedStringBytes,
    /// BIFF formula/name bytes.
    BiffFormulaNameBytes,
    /// BIFF cached text bytes.
    BiffCachedTextBytes,
    /// add-in declarations.
    AddInDeclarations,
    /// BIFF records.
    BiffRecords,
    /// CFB chain sectors.
    CfbChainSectors,
    /// FAT sectors.
    FatSectors,
    /// DIFAT sectors.
    DifatSectors,
    /// FAT entries.
    FatEntries,
    /// mini FAT entries.
    MiniFatEntries,
    /// Workbook stream bytes.
    WorkbookStreamBytes,
    /// CfbFatCapacity capacity.
    CfbFatCapacity,
    /// CfbMiniFatSectors capacity.
    CfbMiniFatSectors,
    /// CfbDifatSectors capacity.
    CfbDifatSectors,
    /// BackendDifatCapacity capacity.
    BackendDifatCapacity,
}
