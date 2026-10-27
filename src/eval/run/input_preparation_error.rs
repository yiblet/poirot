use crate::eval::inputs::InputError;

/// A caller's preparation callback or assignment admission rejected shared inputs.
#[derive(Debug, thiserror::Error)]
pub enum InputPreparationError<E> {
    /// The caller could not produce assignments.
    #[error("could not prepare scenario inputs")]
    Preparation(#[source] E),
    /// Produced assignments violate the source or admission limits.
    #[error("scenario input admission rejected")]
    Admission(#[source] InputError),
}
