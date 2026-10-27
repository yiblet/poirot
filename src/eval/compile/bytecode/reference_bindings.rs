use super::reference_binding::ReferenceBinding;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
/// Occurrence-ordered reference slots and their encoded per-formula coordinates
/// and qualifiers. They can differ while the executable template is shared.
pub struct ReferenceBindings {
    /// Reference descriptors in source occurrence order.
    pub slots: Vec<ReferenceBinding>,
    /// Packed reference coordinates and sheet qualifiers.
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}
