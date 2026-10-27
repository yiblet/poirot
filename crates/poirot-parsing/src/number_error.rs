/// A recognized numeric spelling whose value cannot be represented.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("numeric spelling cannot be represented")]
pub struct NumberError;
