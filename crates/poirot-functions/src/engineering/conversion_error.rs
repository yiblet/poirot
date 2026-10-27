/// Distinguish unknown/incompatible units from arithmetic overflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConversionError {
    /// Unknown tokens, unsupported prefixes or different dimensions.
    #[error("unknown tokens, unsupported prefixes or different dimensions")]
    NotAvailable,
    /// Nonfinite input or result.
    #[error("nonfinite input or result")]
    Number,
}
