use crate::model::{FormulaSource, ModelError};
use poirot_parsing::formula::ParseError;

/// A workbook edit rejected by model validation, identifier comparison, or formula parsing.
/// The formula source is retained when a particular cell or defined name caused failure.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// Source mutation violates canonical model constraints.
    #[error("workbook edit rejected")]
    Model(#[source] ModelError),
    /// Sheet/name comparison requires unsupported identifier mapping.
    #[error("cannot compare identifiers in {target:?}")]
    Identifier {
        /// Formula source being inspected when identifier comparison failed.
        target: FormulaSource,
        /// Underlying checked identifier diagnostic.
        #[source]
        error: crate::model::identifiers::IdentifierError,
    },
    /// Affected formula cannot be parsed or safely rewritten.
    #[error("cannot rewrite formula in {target:?}")]
    Formula {
        /// Cell or defined-name identity of the rejected formula.
        target: FormulaSource,
        /// Underlying formula diagnostic.
        #[source]
        error: ParseError,
    },
    /// Duplicate translation source.
    #[error("duplicate translation source")]
    DuplicateTranslationSource {
        /// Rejected formula identity or name index.
        target: FormulaSource,
    },
    /// Translation source location differs from requested cell.
    #[error("translation source location differs from requested cell")]
    TranslationLocationMismatch {
        /// Rejected formula identity or name index.
        target: FormulaSource,
    },
    /// Translation formula source is absent.
    #[error("translation formula source is absent")]
    MissingTranslationFormula {
        /// Rejected formula identity or name index.
        target: FormulaSource,
    },
    /// Translation workbook identity changed.
    #[error("translation workbook identity changed")]
    StaleTranslationWorkbook,
    /// Translation formula source changed.
    #[error("translation formula source changed")]
    StaleTranslationFormula {
        /// Rejected formula identity or name index.
        target: FormulaSource,
    },
    /// Translation defined name is absent.
    #[error("translation defined name is absent")]
    MissingTranslationName {
        /// Rejected formula identity or name index.
        index: usize,
    },
}
