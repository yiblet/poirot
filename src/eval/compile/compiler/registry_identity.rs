/// The function definitions against which formulas are compiled.
///
/// Extension identity includes IDs, signatures, capabilities, volatility, and name
/// order so incompatible registries cannot reuse compiled programs.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistryIdentity {
    /// Version of the immutable builtin binding contract.
    pub builtin_contract_version: u64,
    /// Complete extension contract identity, when extensions are installed. It is
    /// shared so every prepared formula can keep its policy without a deep copy.
    pub extensions: Option<std::sync::Arc<crate::eval::functions::extensions::ExtensionIdentity>>,
}

impl Default for RegistryIdentity {
    fn default() -> Self {
        Self {
            builtin_contract_version: 21,
            extensions: None,
        }
    }
}

impl RegistryIdentity {
    /// Capture builtins and the supplied registry's exact extension contracts.
    pub fn with_extensions(
        registry: &crate::eval::functions::extensions::ExtensionRegistry,
    ) -> Self {
        Self {
            extensions: Some(std::sync::Arc::new(registry.identity())),
            ..Self::default()
        }
    }
}
