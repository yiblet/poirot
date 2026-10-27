use super::extension_id::ExtensionId;
use super::extension_registration_identity::ExtensionRegistrationIdentity;

/// A specific registration selected while binding a formula. Registry replacement
/// creates a new binding; old plans must be rebound before using the replacement.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionBinding {
    /// Opaque executable registration selected during binding.
    pub id: ExtensionId,
    /// Exact registration contract guarding execution.
    pub registration: ExtensionRegistrationIdentity,
}
