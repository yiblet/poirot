use crate::IoError;

/// Backend failures distinguish content needing transient adaptation from input errors.
#[derive(Debug, thiserror::Error)]
pub(crate) enum XlsxDecodeError {
    /// Backend content requires transient format adaptation.
    #[error("XLSX backend requires adaptation")]
    NeedsAdapter,
    /// Backend or model rejected the input.
    #[error("XLSX input rejected")]
    Input(#[source] IoError),
}
impl XlsxDecodeError {
    /// Convert a terminal backend failure to the public I/O error.
    pub(crate) fn into_io(self) -> IoError {
        match self {
            Self::NeedsAdapter => {
                IoError::Read(crate::WorkbookReadError::UnsupportedCellContentAfterAdaptation)
            }
            Self::Input(error) => error,
        }
    }
}
impl From<IoError> for XlsxDecodeError {
    fn from(error: IoError) -> Self {
        Self::Input(error)
    }
}
impl From<crate::ModelError> for XlsxDecodeError {
    fn from(error: crate::ModelError) -> Self {
        Self::Input(error.into())
    }
}
