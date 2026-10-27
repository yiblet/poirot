//! Comparable identity of a retained external function namespace.

/// Captured case-policy version and first stored function spellings for exact namespace comparison.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExternalFunctionNamespaceIdentity {
    /// Version of the case comparison contract.
    pub case_policy_version: u64,
    /// Exact first stored spellings, ordered by ASCII-insensitive key.
    pub stored_names: Vec<String>,
}
