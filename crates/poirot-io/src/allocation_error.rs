/// Allocation refusal while constructing workbook data.
#[derive(Debug, thiserror::Error)]
pub enum AllocationError {
    /// File buffer allocation was refused by the operating system.
    #[error("workbook file buffer allocation refused")]
    FileBuffer {
        /// Underlying out-of-memory IO failure.
        #[source]
        source: std::io::Error,
    },
    /// Export style identities could not be reserved.
    #[error("export style identity allocation refused")]
    ExportStyles(#[source] std::collections::TryReserveError),
    /// Imported style table could not be reserved.
    #[error("style table allocation refused")]
    StyleTable(#[source] std::collections::TryReserveError),
    /// Shared visual style references could not be reserved.
    #[error("shared visual style allocation refused")]
    SharedStyles(#[source] std::collections::TryReserveError),
    /// Spill formula export storage could not be reserved.
    #[error("spill formula export allocation refused")]
    SpillFormula(#[source] std::collections::TryReserveError),
}
