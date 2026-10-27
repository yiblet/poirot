use crate::eval::limits::EvaluationError;
use crate::model::ModelError;

/// Failure to construct, restore, or operate on an overlay.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// Invalid workbook target or authored cell content.
    #[error("overlay model operation rejected")]
    Model(#[source] ModelError),
    /// Operational evaluation or policy failure.
    #[error("overlay evaluation rejected")]
    Evaluation(#[source] EvaluationError),
    /// Invalid or incompatible serialized state.
    #[error("overlay snapshot rejected")]
    Snapshot(#[source] super::overlay_snapshot_error::OverlaySnapshotError),
    /// The supplied extension registry does not match the snapshot's identity.
    #[error("overlay requires its exact extension registry")]
    RegistryMismatch,
}

impl From<ModelError> for OverlayError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}

impl From<EvaluationError> for OverlayError {
    fn from(e: EvaluationError) -> Self {
        Self::Evaluation(e)
    }
}

impl From<crate::eval::reuse::CacheError> for OverlayError {
    fn from(error: crate::eval::reuse::CacheError) -> Self {
        Self::Snapshot(error.into())
    }
}

impl From<poirot_io::ModelError> for OverlayError {
    fn from(error: poirot_io::ModelError) -> Self {
        Self::Model(error.into())
    }
}
