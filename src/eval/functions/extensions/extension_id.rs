/// Opaque executable registration ID assigned by a registry; not a stable serialized function name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ExtensionId(pub(super) u64);
