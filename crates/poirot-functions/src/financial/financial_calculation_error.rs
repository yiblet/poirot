use super::financial_error::FinancialError as E;
/// Distinguish spreadsheet errors from host work exhaustion and bounded numerical refusal.
#[derive(Debug, PartialEq, thiserror::Error)]
pub enum FinancialCalculationError<F> {
    /// The mathematical domain or finite result is invalid.
    #[error("the mathematical domain or finite result is invalid")]
    Spreadsheet(#[source] E),
    /// The host stopped numerical work.
    #[error("the host stopped numerical work")]
    Operational(#[source] F),
    /// The independently bounded algorithm cannot admit this workload.
    #[error("the independently bounded algorithm cannot admit this workload")]
    ComputationalLimit,
}
