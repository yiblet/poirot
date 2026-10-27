//! Unit tests for formula preparation, template pooling and spill classification.

/// Allocation counts of bytecode compilation and classified template hits.
mod allocations;
/// Plan snapshot round trips and rejection of corrupt plans.
mod plan_snapshot;
/// Shared template pools across sessions and registries.
mod pool;
/// Point-reference scalar matching and its spill proofs.
mod scalar_matcher;
/// Spill classification of prepared formulas.
mod spill_classification;
/// Template reuse and rebinding of executable programs.
mod template;
/// Template admission under preparation limits.
mod template_admission;
