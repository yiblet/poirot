//! Unit tests for incremental overlay reuse, edits and flushing.

/// Batched edits against sequential edits and flushing.
mod batch_edits;
/// Value-ordered dependency cutoffs and lazy branch reuse.
mod conditional_dependencies;
/// Range membership replacing redundant point dependencies.
mod covered_range_dependencies;
/// Spill discovery and lookup prefixes restored from snapshots.
mod discovery_restoration;
/// Parity of editable direct workspaces and persistent overlays.
mod dual_evaluation;
/// Edit transitions, explicit blanks and baseline inheritance.
mod edit_states;
/// Random identity, spill conflicts and cache hits across warm and restored runs.
mod evaluation_semantics;
/// Pending edit flushing and its invalidation.
mod flush;
/// Incremental reuse, dependency invalidation, snapshots and lookup indices.
mod incremental;
/// Export buffers, rejected cache commits and borrow conflicts of overlay evaluations.
mod overlay_evaluation_safety;
/// Inclusive rectangle invalidation across axes and sheets.
mod range_invalidation_bounds;
/// Context-sensitive invalidation of retained results.
mod retained_context;
/// Spill-sensitive coordinates as incremental nodes.
mod spill_dependencies;
/// Reset of baseline spill producers and classification proofs.
mod spill_reset;
/// Rejection of malformed style-only markers in snapshots.
mod style_marker_validation;
