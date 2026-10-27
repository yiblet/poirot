use crate::eval::limits::EvaluationFailure;
use crate::model::ModelError;

/// Failure to construct, restore, or operate on an overlay.
#[derive(Debug)]
pub enum OverlayError {
    /// Invalid workbook target or authored cell content.
    Model(ModelError),
    /// Operational evaluation or policy failure.
    Evaluation(EvaluationFailure),
    /// Invalid or incompatible serialized state.
    Snapshot(String),
    /// The supplied extension registry does not match the snapshot's identity.
    RegistryMismatch,
}

impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(e) => e.fmt(f),
            Self::Evaluation(e) => e.fmt(f),
            Self::Snapshot(e) => write!(f, "overlay snapshot: {e}"),
            Self::RegistryMismatch => f.write_str("overlay requires its exact extension registry"),
        }
    }
}

impl std::error::Error for OverlayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Model(error) => Some(error),
            Self::Evaluation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ModelError> for OverlayError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}

impl From<EvaluationFailure> for OverlayError {
    fn from(e: EvaluationFailure) -> Self {
        Self::Evaluation(e)
    }
}
