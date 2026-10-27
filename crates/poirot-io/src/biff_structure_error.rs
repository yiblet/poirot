/// Typed conditions reported by BiffStructure operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BiffStructureError {
    /// Integer offset overflow.
    #[error("integer offset overflow")]
    IntegerOffsetOverflow,
    /// Truncated cell record.
    #[error("truncated cell record")]
    TruncatedCellRecord,
    /// MulRK length.
    #[error("MulRK length")]
    MulrkLength,
    /// MulRK coordinates/length.
    #[error("MulRK coordinates/length")]
    MulrkCoordinatesLength,
    /// Formula cached result type.
    #[error("formula cached result type")]
    FormulaCachedResultType,
    /// Invalid BIFF worksheet visibility.
    #[error("invalid BIFF worksheet visibility")]
    InvalidBiffWorksheetVisibility,
    /// Truncated SST counts.
    #[error("truncated SST counts")]
    TruncatedSstCounts,
    /// Truncated shared string.
    #[error("truncated shared string")]
    TruncatedSharedString,
    /// Missing string continuation.
    #[error("missing string continuation")]
    MissingStringContinuation,
    /// Empty string continuation.
    #[error("empty string continuation")]
    EmptyStringContinuation,
    /// Truncated string extension.
    #[error("truncated string extension")]
    TruncatedStringExtension,
    /// Missing SST.
    #[error("missing SST")]
    MissingSst,
    /// Truncated string characters.
    #[error("truncated string characters")]
    TruncatedStringCharacters,
    /// Split UTF16 code unit.
    #[error("split UTF16 code unit")]
    SplitUtf16CodeUnit,
    /// Missing cached STRING.
    #[error("missing cached STRING")]
    MissingCachedString,
    /// Missing byte.
    #[error("missing byte")]
    MissingByte,
    /// Missing cached STRING continuation.
    #[error("missing cached STRING continuation")]
    MissingCachedStringContinuation,
    /// Truncated BIFF token/string.
    #[error("truncated BIFF token/string")]
    TruncatedBiffTokenString,
    /// EXTERNSHEET record length.
    #[error("EXTERNSHEET record length")]
    ExternsheetRecordLength,
    /// Sheet boundary.
    #[error("sheet boundary")]
    SheetBoundary,
    /// Shared string index.
    #[error("shared string index")]
    SharedStringIndex,
    /// Cached STRING without FORMULA.
    #[error("cached STRING without FORMULA")]
    CachedStringWithoutFormula,
    /// NameX slot.
    #[error("NameX slot")]
    NamexSlot,
    /// Unknown BIFF error code.
    #[error("unknown BIFF error code")]
    UnknownBiffErrorCode,
    /// Formula stack underflow.
    #[error("formula stack underflow")]
    FormulaStackUnderflow,
    /// Number token.
    #[error("number token")]
    NumberToken,
    /// Name token.
    #[error("name token")]
    NameToken,
    /// NameX XTI.
    #[error("NameX XTI")]
    NamexXti,
    /// External function must use FuncVar.
    #[error("external function must use FuncVar")]
    ExternalFunctionMustUseFuncvar,
    /// External function stack.
    #[error("external function stack")]
    ExternalFunctionStack,
    /// External function arity.
    #[error("external function arity")]
    ExternalFunctionArity,
    /// External function identity.
    #[error("external function identity")]
    ExternalFunctionIdentity,
    /// Nested external function identity.
    #[error("nested external function identity")]
    NestedExternalFunctionIdentity,
    /// Function arity.
    #[error("function arity")]
    FunctionArity,
    /// Function argument stack.
    #[error("function argument stack")]
    FunctionArgumentStack,
    /// Formula final stack.
    #[error("formula final stack")]
    FormulaFinalStack,
    /// Array number.
    #[error("array number")]
    ArrayNumber,
    /// Unknown array constant type.
    #[error("unknown array constant type")]
    UnknownArrayConstantType,
    /// Truncated integer.
    #[error("truncated integer")]
    TruncatedInteger,
    /// Record offset overflow.
    #[error("record offset overflow")]
    RecordOffsetOverflow,
    /// Truncated record.
    #[error("truncated record")]
    TruncatedRecord,
    /// Sector overflow.
    #[error("sector overflow")]
    SectorOverflow,
    /// Sector offset overflow.
    #[error("sector offset overflow")]
    SectorOffsetOverflow,
    /// Sector beyond input.
    #[error("sector beyond input")]
    SectorBeyondInput,
    /// Cyclic sector chain.
    #[error("cyclic sector chain")]
    CyclicSectorChain,
    /// Mini sector overflow.
    #[error("mini sector overflow")]
    MiniSectorOverflow,
    /// Mini sector offset overflow.
    #[error("mini sector offset overflow")]
    MiniSectorOffsetOverflow,
    /// Mini sector beyond root stream.
    #[error("mini sector beyond root stream")]
    MiniSectorBeyondRootStream,
    /// Truncated stream block.
    #[error("truncated stream block")]
    TruncatedStreamBlock,
    /// Sector has no FAT entry.
    #[error("sector has no FAT entry")]
    SectorHasNoFatEntry,
    /// Stream chain shorter than declared length.
    #[error("stream chain shorter than declared length")]
    StreamChainShorterThanDeclaredLength,
    /// CFB signature.
    #[error("CFB signature")]
    CfbSignature,
    /// Sector size.
    #[error("sector size")]
    SectorSize,
    /// CFB header.
    #[error("CFB header")]
    CfbHeader,
    /// Cyclic DIFAT.
    #[error("cyclic DIFAT")]
    CyclicDifat,
    /// Missing root directory.
    #[error("missing root directory")]
    MissingRootDirectory,
    /// Mini FAT size overflow.
    #[error("mini FAT size overflow")]
    MiniFatSizeOverflow,
    /// Missing Workbook/Book stream.
    #[error("missing Workbook/Book stream")]
    MissingWorkbookBookStream,
    /// Missing SST record.
    #[error("missing SST record")]
    MissingSstRecord,
    /// ROW length.
    #[error("ROW length")]
    RowLength,
    /// Merge count exceeds record.
    #[error("merge count exceeds record")]
    MergeCountExceedsRecord,
    /// Dimensions length.
    #[error("Dimensions length")]
    DimensionsLength,
    /// Formula token claim exceeds record.
    #[error("formula token claim exceeds record")]
    FormulaTokenClaimExceedsRecord,
    /// Truncated metadata record.
    #[error("truncated metadata record")]
    TruncatedMetadataRecord,
    /// Name token/string length.
    #[error("name token/string length")]
    NameTokenStringLength,
    /// Truncated defined-name reference token.
    #[error("truncated defined-name reference token")]
    TruncatedDefinedNameReferenceToken,
    /// Sheet offset is not a record boundary.
    #[error("sheet offset is not a record boundary")]
    SheetOffsetIsNotARecordBoundary,
}
