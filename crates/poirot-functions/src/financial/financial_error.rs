/// Spreadsheet failures produced by financial kernels after adapter coercion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FinancialError {
    /// An operand cannot satisfy the function's value contract.
    #[error("an operand cannot satisfy the function's value contract")]
    Value,
    /// Numeric domain, overflow, or convergence failure.
    #[error("numeric domain, overflow, or convergence failure")]
    Num,
    /// A financial expression has a zero denominator.
    #[error("a financial expression has a zero denominator")]
    DivZero,
}
