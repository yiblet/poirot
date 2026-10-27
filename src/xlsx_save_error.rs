/// Failure to capture effective source or export it as an XLSX workbook.
#[derive(Debug, thiserror::Error)]
pub enum XlsxSaveError {
    /// Effective scenario edits could not be assembled.
    #[error("could not capture effective workbook source")]
    Model(#[from] crate::model::ModelError),
    /// XLSX validation, encoding, or filesystem output failed.
    #[error("could not export xlsx workbook")]
    Io(#[from] poirot_io::IoError),
}
