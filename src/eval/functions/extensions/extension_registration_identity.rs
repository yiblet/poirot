use super::extension_contract::ExtensionContract;
use super::extension_id::ExtensionId;
use super::extension_namespace::ExtensionNamespace;

/// One registration's executable identity and declared behavior.
/// Change `implementation_signature` whenever implementation or captured configuration changes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionRegistrationIdentity {
    /// Exact opaque executable binding identity, including insertion assignment.
    pub binding_id: ExtensionId,
    /// Canonical registered spelling.
    pub name: String,
    /// User-defined or ATP registration namespace.
    pub namespace: ExtensionNamespace,
    /// Caller supplied revision of the implementation AND captured configuration.
    pub implementation_signature: String,
    /// Declared execution behavior and host access.
    pub contract: ExtensionContract,
}
