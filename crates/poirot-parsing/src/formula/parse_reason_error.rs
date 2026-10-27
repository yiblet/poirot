/// Specific reason formula parsing, printing, or translation was rejected.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
pub enum ParseReasonError {
    /// Parse policy must tighten canonical bounds.
    #[error("parse policy must tighten canonical bounds")]
    InvalidPolicy {
        /// Rejected parser bounds.
        policy: super::metadata::FormulaParsePolicy,
    },
    /// Reference exceeds configured grid policy.
    #[error("reference exceeds configured grid policy")]
    ConfiguredGrid {
        /// Configured exclusive row bound.
        max_rows: u32,
        /// Configured exclusive column bound.
        max_columns: u32,
    },
    /// Unexpected trailing input.
    #[error("unexpected trailing input")]
    TrailingInput,
    /// Invalid UTF-8 parser cursor.
    #[error("invalid utf-8 parser cursor")]
    InvalidCursor,
    /// Expression complexity limit exceeded.
    #[error("expression complexity limit exceeded")]
    ExpressionLimit {
        /// Maximum expression count.
        limit: usize,
        /// Expressions already admitted.
        admitted: usize,
    },
    /// Expression nesting limit exceeded.
    #[error("expression nesting limit exceeded")]
    NestingLimit {
        /// Maximum nesting depth.
        limit: usize,
        /// Current nesting depth.
        depth: usize,
    },
    /// Unterminated quoted text.
    #[error("unterminated quoted text")]
    UnterminatedQuote,
    /// External workbook sheet spans are unsupported.
    #[error("external workbook sheet spans are unsupported")]
    ExternalSheetSpan,
    /// Unknown error literal.
    #[error("unknown error literal")]
    UnknownErrorLiteral,
    /// Invalid number.
    #[error("invalid number")]
    InvalidNumber,
    /// Number is not finite.
    #[error("number is not finite")]
    NonfiniteNumber,
    /// Expected expression.
    #[error("expected expression")]
    ExpectedExpression,
    /// Repeated sheet qualifier.
    #[error("repeated sheet qualifier")]
    RepeatedQualifier,
    /// Invalid function name.
    #[error("invalid function name")]
    InvalidFunctionName,
    /// Invalid absolute reference.
    #[error("invalid absolute reference")]
    InvalidAbsoluteReference,
    /// Invalid sheet reference.
    #[error("invalid sheet reference")]
    InvalidSheetReference,
    /// Expected bounded sheet-span cell.
    #[error("expected bounded sheet-span cell")]
    ExpectedSpanCell,
    /// Sheet spans require bounded cell references.
    #[error("sheet spans require bounded cell references")]
    UnboundedSpanReference,
    /// Array elements must be constants.
    #[error("array elements must be constants")]
    NonconstantArray,
    /// Array rows have different lengths.
    #[error("array rows have different lengths")]
    RaggedArray,
    /// AST cannot be preserved by canonical syntax.
    #[error("ast cannot be preserved by canonical syntax")]
    UnrepresentableAst,
    /// Column exceeds canonical grid.
    #[error("column exceeds canonical grid")]
    ColumnOutsideGrid {
        /// Rejected zero-based column.
        column: u32,
    },
    /// Printer complexity limit exceeded.
    #[error("printer complexity limit exceeded")]
    PrinterLimit {
        /// Observed nesting depth.
        depth: usize,
        /// Observed expression count.
        nodes: usize,
    },
    /// Invalid bounded sheet-span reference.
    #[error("invalid bounded sheet-span reference")]
    InvalidSheetSpan,
    /// Row overflow.
    #[error("row overflow")]
    RowOverflow,
    /// Formula location exceeds translation grid.
    #[error("formula location exceeds translation grid")]
    TranslationLocationOutsideGrid {
        /// Rejected zero-based row.
        row: u32,
        /// Rejected zero-based column.
        column: u32,
        /// Exclusive row bound.
        max_rows: u32,
        /// Exclusive column bound.
        max_columns: u32,
    },
    /// Stale formula translation source.
    #[error("stale formula translation source")]
    StaleTranslationSource,
    /// Unsupported deleted reference: translation crosses grid boundary.
    #[error("unsupported deleted reference: translation crosses grid boundary")]
    TranslationOutsideGrid {
        /// Original coordinate.
        coordinate: u32,
        /// Rejected translated coordinate.
        translated: i64,
        /// Exclusive axis bound.
        bound: u32,
    },
    /// Unsupported area endpoint reordering.
    #[error("unsupported area endpoint reordering")]
    TranslationReordersEndpoints,
    /// Sheet spans require cells or paired whole-axis references.
    #[error("sheet spans require cells or paired whole-axis references")]
    IncompatibleSpanEndpoints,
    /// Cell reference exceeds Excel grid.
    #[error("cell reference exceeds excel grid")]
    CellOutsideGrid,
    /// Incompatible primitive range endpoints.
    #[error("incompatible primitive range endpoints")]
    IncompatibleRangeEndpoints,
    /// Invalid range endpoint.
    #[error("invalid range endpoint")]
    InvalidRangeEndpoint,
    /// A required grammar delimiter was absent.
    #[error("expected '{expected}'")]
    ExpectedCharacter {
        /// Required delimiter.
        expected: char,
    },
}
