//! Frozen HSSF external function names, supplied by an importer or explicit caller.
//! LinkTable reuses the first stored spelling before asking the runtime UDF finder.

use super::external_function_namespace_identity::ExternalFunctionNamespaceIdentity;
use super::external_name_lookup::ExternalNameLookup;
use super::external_namespace_error::ExternalNamespaceError;
use std::collections::BTreeMap;

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
