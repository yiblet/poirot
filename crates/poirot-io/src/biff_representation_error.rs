/// Typed conditions reported by BiffRepresentation operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BiffRepresentationError {
    /// BIFF error code.
    #[error("BIFF error code")]
    BiffErrorCode,
    /// Cell coordinate.
    #[error("cell coordinate")]
    CellCoordinate,
    /// Only BIFF8 (Excel 97-2003) XLS workbooks are supported.
    #[error("only BIFF8 (Excel 97-2003) XLS workbooks are supported")]
    UnsupportedBiffVersion,
    /// Decoded string bytes overflow.
    #[error("decoded string bytes overflow")]
    DecodedStringBytesOverflow,
    /// Rich/extended cached STRING records are not supported.
    #[error("rich/extended cached STRING records are not supported")]
    RichExtendedCachedStringRecordsAreNotSupported,
    /// BIFF syntax text contains non-scalar UTF16.
    #[error("BIFF syntax text contains non-scalar UTF16")]
    BiffSyntaxTextContainsNonScalarUtf16,
    /// BIFF text total overflow.
    #[error("BIFF text total overflow")]
    BiffTextTotalOverflow,
    /// XTI count overflow.
    #[error("XTI count overflow")]
    XtiCountOverflow,
    /// Text coordinate.
    #[error("text coordinate")]
    TextCoordinate,
    /// Formula coordinate.
    #[error("formula coordinate")]
    FormulaCoordinate,
    /// Encrypted XLS workbooks are not supported.
    #[error("encrypted XLS workbooks are not supported")]
    EncryptedXls,
    /// Add-in count overflow.
    #[error("add-in count overflow")]
    AddInCountOverflow,
    /// Add-in bytes overflow.
    #[error("add-in bytes overflow")]
    AddInBytesOverflow,
    /// Unsupported add-in EXTERNNAME flags.
    #[error("unsupported add-in EXTERNNAME flags")]
    UnsupportedAddInExternnameFlags,
    /// Add-in projection overflow.
    #[error("add-in projection overflow")]
    AddInProjectionOverflow,
    /// Unsupported add-in EXTERNNAME formula.
    #[error("unsupported add-in EXTERNNAME formula")]
    UnsupportedAddInExternnameFormula,
    /// External NameX scope unsupported.
    #[error("external NameX scope unsupported")]
    ExternalNamexScopeUnsupported,
    /// External workbook/UDF NameX unsupported.
    #[error("external workbook/UDF NameX unsupported")]
    ExternalWorkbookUdfNamexUnsupported,
    /// Unsupported add-in function identity.
    #[error("unsupported add-in function identity")]
    UnsupportedAddInFunctionIdentity,
    /// NameX used outside external function call.
    #[error("NameX used outside external function call")]
    NamexUsedOutsideExternalFunctionCall,
    /// Formula token count overflow.
    #[error("formula token count overflow")]
    FormulaTokenCountOverflow,
    /// PtgExtraMem size overflow.
    #[error("PtgExtraMem size overflow")]
    PtgextramemSizeOverflow,
    /// UDF function identity unsupported.
    #[error("UDF function identity unsupported")]
    UdfFunctionIdentityUnsupported,
    /// External formula size overflow.
    #[error("external formula size overflow")]
    ExternalFormulaSizeOverflow,
    /// Formula text overflow.
    #[error("formula text overflow")]
    FormulaTextOverflow,
    /// NameX used as ordinary argument.
    #[error("NameX used as ordinary argument")]
    NamexUsedAsOrdinaryArgument,
    /// Array constant size overflow.
    #[error("array constant size overflow")]
    ArrayConstantSizeOverflow,
    /// Stream length exceeds address space.
    #[error("stream length exceeds address space")]
    StreamLengthExceedsAddressSpace,
}
