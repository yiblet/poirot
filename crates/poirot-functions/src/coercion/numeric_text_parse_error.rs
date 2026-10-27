use crate::coercion::{NumericTextError, NumericTextSourceError};

/// NUMBERVALUE failure preserving spreadsheet errors versus operational refusals.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum NumericTextParseError {
    /// Catchable numeric-text parsing error.
    #[error("numeric text returned a spreadsheet error")]
    Spreadsheet(#[source] NumericTextError),
    /// Pinned source fault retained for operational handling.
    #[error("numeric text source parsing failed")]
    Source(#[source] NumericTextSourceError),
}
