//! Workbook-scoped HSSF external function names retained before runtime UDF lookup.
//!
//! Depends on: no other eval folder.

/// Retained first spellings and their lookup.
mod external_function_namespace;

/// Comparable identity of a retained namespace.
mod external_function_namespace_identity;

/// Outcome of one external name lookup.
mod external_name_lookup;

/// Failures while retaining external names.
mod external_namespace_error;

pub use external_function_namespace::ExternalFunctionNamespace;
pub use external_function_namespace_identity::ExternalFunctionNamespaceIdentity;
pub use external_name_lookup::ExternalNameLookup;
pub use external_namespace_error::ExternalNamespaceError;
