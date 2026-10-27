/// Failure of canonical fixture workbook preparation.
#[derive(Debug, thiserror::Error)]
pub(super) enum WorkbookPreparationError {
    /// Fixture workbook mutation failed.
    #[error("fixture workbook mutation failed")]
    Model(#[from] poirot::model::ModelError),
    /// Missing worksheet.
    #[error("fixture worksheet {index} is missing")]
    MissingWorksheet {
        /// Zero-based worksheet index.
        index: usize,
    },
    /// Rejected name.
    #[error("rejected declaration {name:?}; non-use not proved")]
    RejectedName {
        /// Rejected defined-name spelling.
        name: String,
        /// Non-use proof failure.
        #[source]
        source: super::name_admission_error::NameAdmissionError,
    },
}
