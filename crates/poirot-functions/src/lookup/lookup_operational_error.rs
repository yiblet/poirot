/// Host execution failures kept outside spreadsheet error handling.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum LookupOperationalError {
    /// Host refused the requested work reservation.
    #[error("lookup work budget exceeded")]
    BudgetExceeded,
    /// Resource-size arithmetic overflowed before allocation.
    #[error("lookup resource size overflowed")]
    ResourceOverflow,
    /// Host cancelled evaluation.
    #[error("lookup evaluation cancelled")]
    Cancelled,
    /// Required host service is unavailable.
    #[error("lookup host service unavailable")]
    HostUnavailable,
}
