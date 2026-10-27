//! Shared fixtures for the run contract tests.

mod admission_preparation;
mod cache_completion;
/// Cached worksheet reads, memo ownership and metered failures.
mod cached_reads;
mod consumed_dependencies;
/// Effective sparse inputs, shared sources and run isolation.
mod effective_inputs;
mod execution_configuration;
/// Memo retention admission and partial-run limits.
mod memo_retention;
/// Checked constructors that combine borrowed run settings and policies.
mod session_configuration;
/// Shared fixtures for run tests.
mod support;
