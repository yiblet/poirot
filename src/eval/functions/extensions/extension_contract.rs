use super::extension_cacheability::ExtensionCacheability;
use super::extension_capabilities::ExtensionCapabilities;
use super::extension_evaluation_strategy::ExtensionEvaluationStrategy;
use crate::eval::functions::registry::Arity;

/// A provider's accepted arity, evaluation order, capabilities, and reuse contract.
/// Declare all workbook/context access the implementation may request; unsupported
/// access aborts operationally instead of silently inventing data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExtensionContract {
    /// Accepted syntactic argument count.
    pub arity: Arity,
    /// Ordering of argument resolution and arity validation.
    pub evaluation: ExtensionEvaluationStrategy,
    /// Host access the provider is permitted to request.
    pub capabilities: ExtensionCapabilities,
    /// Explicit evidence permitting reuse, independent of provider thread safety.
    pub cacheability: ExtensionCacheability,
}
