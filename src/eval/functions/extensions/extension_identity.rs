use super::extension_registration_identity::ExtensionRegistrationIdentity;

/// Exact registry snapshot used to bind and validate prepared formulas.
/// Includes registration revisions and external-name identity, not just function names.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionIdentity {
    /// Registry revision after successful namespace or registration updates.
    pub epoch: u64,
    /// Ordered external function namespace, when installed.
    pub external_namespace:
        Option<crate::eval::functions::external_namespace::ExternalFunctionNamespaceIdentity>,
    /// Exact registration identities in registry order.
    pub registrations: Vec<ExtensionRegistrationIdentity>,
}
