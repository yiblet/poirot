use std::any::Any;

/// Opaque executable payload retained by an internal callable value.
/// The VM owns the concrete payload and is the only component that invokes it.
pub(in crate::eval) trait CallableBody: Any + Send + Sync {}
