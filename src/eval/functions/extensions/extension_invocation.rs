use super::extension_binding::ExtensionBinding;
use crate::eval::coordinates::EvaluationTarget;

/// Records the exact extension contract entered by one formula cell.
/// Consumed-read tracking uses this binding to guard cached results when a provider
/// or its declared behavior changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionInvocation {
    /// Formula cell that entered the provider.
    pub target: EvaluationTarget,
    /// Exact executed registration and declared contract.
    pub binding: ExtensionBinding,
}
