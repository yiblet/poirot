//! Ownership of the runtime extension registry attached to a source.

use std::sync::Arc;

/// Runtime provider owner, excluded from serialized binding identity.
pub(super) enum RegistryHandle<'a> {
    /// Caller-managed immutable registry.
    Borrowed(&'a crate::eval::functions::extensions::ExtensionRegistry),
    /// Providers shared with worker-owned workspaces.
    Shared(Arc<crate::eval::functions::extensions::ExtensionRegistry>),
}

impl RegistryHandle<'_> {
    /// Read implementations independently of handle ownership.
    pub fn get(&self) -> &crate::eval::functions::extensions::ExtensionRegistry {
        match self {
            Self::Borrowed(registry) => registry,
            Self::Shared(registry) => registry,
        }
    }
}
