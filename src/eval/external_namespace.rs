//! Frozen HSSF external function names, supplied by an importer or explicit caller.
//! LinkTable reuses the first stored spelling before asking the runtime UDF finder.
use std::collections::BTreeMap;

/// Captured case-policy version and first stored function spellings for exact namespace comparison.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExternalFunctionNamespaceIdentity {
    /// Version of the case comparison contract.
    pub case_policy_version: u64,
    /// Exact first stored spellings, ordered by ASCII-insensitive key.
    pub stored_names: Vec<String>,
}
/// A stored external function name is unsupported, or its declared retention limit was exceeded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalNamespaceError {
    /// Spelling requires an unsupported identifier grammar or case policy.
    UnsupportedName,
    /// Input entry count exceeded the declared retention limit.
    ResourceLimit,
}
impl std::fmt::Display for ExternalNamespaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "external function namespace: {self:?}")
    }
}
impl std::error::Error for ExternalNamespaceError {}
/// A preserved first spelling, definite absence, or an unsupported name comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalNameLookup<'a> {
    /// Exact first stored spelling selected by case-insensitive lookup.
    Stored(&'a str),
    /// Supported lookup found no stored spelling.
    Absent,
    /// No Java-compatible Unicode case policy is installed; do not guess a match.
    UnsupportedName,
}
/// Workbook-scoped HSSF external function spellings, retained before runtime UDF lookup.
/// This namespace preserves first-spelling identity; it neither loads external data
/// nor implements the referenced functions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalFunctionNamespace {
    names: BTreeMap<String, String>,
}
impl ExternalFunctionNamespace {
    /// Bound input entries, including duplicates, before retaining any namespace.
    /// This does not scan formulas or infer external names from function spellings.
    pub fn from_stored_names<I, S>(
        names: I,
        maximum_entries: usize,
    ) -> Result<Self, ExternalNamespaceError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut stored = BTreeMap::new();
        for (index, name) in names.into_iter().enumerate() {
            if index >= maximum_entries {
                return Err(ExternalNamespaceError::ResourceLimit);
            }
            let name = name.as_ref();
            if name.is_empty()
                || !name.is_ascii()
                || name.len() > 255
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
            {
                return Err(ExternalNamespaceError::UnsupportedName);
            }
            stored
                .entry(name.to_ascii_uppercase())
                .or_insert_with(|| name.to_owned());
        }
        Ok(Self { names: stored })
    }
    /// Resolve a spelling while preserving stored case and unsupported comparisons.
    pub fn lookup(&self, name: &str) -> ExternalNameLookup<'_> {
        if !name.is_ascii() || name.len() > 255 {
            return ExternalNameLookup::UnsupportedName;
        }
        match self.names.get(&name.to_ascii_uppercase()) {
            Some(stored) => ExternalNameLookup::Stored(stored),
            None => ExternalNameLookup::Absent,
        }
    }
    /// Capture exact ordered spellings and their case-policy version.
    pub fn identity(&self) -> ExternalFunctionNamespaceIdentity {
        ExternalFunctionNamespaceIdentity {
            case_policy_version: 1,
            stored_names: self.names.values().cloned().collect(),
        }
    }
}
