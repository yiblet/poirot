use super::parse_error_kind::ParseErrorKind;
use super::parse_reason_error::ParseReasonError;

/// Formula rejection with a UTF-8 source position and inspectable cause.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, thiserror::Error)]
#[error("formula rejected at byte {position}")]
pub struct ParseError {
    /// UTF-8 byte offset of the rejected input.
    pub position: usize,
    /// Specific syntax, policy, printing, or translation rejection.
    #[source]
    pub reason: ParseReasonError,
}

impl ParseError {
    /// Classify rejection without inspecting its display text.
    pub fn kind(&self) -> ParseErrorKind {
        match self.reason {
            ParseReasonError::InvalidPolicy { .. } => ParseErrorKind::InvalidPolicy,
            ParseReasonError::ExpressionLimit { .. }
            | ParseReasonError::NestingLimit { .. }
            | ParseReasonError::ConfiguredGrid { .. }
            | ParseReasonError::PrinterLimit { .. } => ParseErrorKind::ResourceLimit,
            ParseReasonError::TrailingInput
            | ParseReasonError::InvalidCursor
            | ParseReasonError::UnterminatedQuote
            | ParseReasonError::ExternalSheetSpan
            | ParseReasonError::UnknownErrorLiteral
            | ParseReasonError::InvalidNumber
            | ParseReasonError::NonfiniteNumber
            | ParseReasonError::ExpectedExpression
            | ParseReasonError::RepeatedQualifier
            | ParseReasonError::InvalidFunctionName
            | ParseReasonError::InvalidAbsoluteReference
            | ParseReasonError::InvalidSheetReference
            | ParseReasonError::ExpectedSpanCell
            | ParseReasonError::UnboundedSpanReference
            | ParseReasonError::NonconstantArray
            | ParseReasonError::RaggedArray
            | ParseReasonError::UnrepresentableAst
            | ParseReasonError::ColumnOutsideGrid { .. }
            | ParseReasonError::InvalidSheetSpan
            | ParseReasonError::RowOverflow
            | ParseReasonError::TranslationLocationOutsideGrid { .. }
            | ParseReasonError::StaleTranslationSource
            | ParseReasonError::TranslationOutsideGrid { .. }
            | ParseReasonError::TranslationReordersEndpoints
            | ParseReasonError::IncompatibleSpanEndpoints
            | ParseReasonError::CellOutsideGrid
            | ParseReasonError::IncompatibleRangeEndpoints
            | ParseReasonError::InvalidRangeEndpoint
            | ParseReasonError::ExpectedCharacter { .. } => ParseErrorKind::Syntax,
        }
    }
}
