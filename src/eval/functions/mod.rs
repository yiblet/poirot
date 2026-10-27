//! Function catalog: builtin identities, spellings and dispatch metadata, registered
//! extension functions, workbook-scoped external function names, and Analysis ToolPak
//! name lists.
//!
//! Depends on: config (builtin semantics and evaluation inputs), coordinates (calling
//! cell targets), limits (evaluation failures) and values (scalar results and function
//! failures).

/// Pinned Analysis ToolPak supported and placeholder function names.
mod atp;

/// Explicit registration and execution contracts for extension functions.
pub mod extensions;

/// Workbook-scoped external function names retained before runtime UDF lookup.
pub mod external_namespace;

/// Builtin handler identities, spellings, arity, and dependency policies.
pub mod registry;
