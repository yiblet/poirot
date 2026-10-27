//! Retain model edits and reuse unaffected calculations across scenarios.
//!
//! An overlay keeps edits, compiled formulas, intermediate results, and dependency
//! information between calculations. Input changes invalidate affected results;
//! unrelated results remain available. Context changes and volatile functions
//! also participate in invalidation. The baseline changes only when edits are
//! explicitly applied or flushed; snapshots can save state for later restoration.
//!
//! ```
//! use poirot::eval::{EvaluationContext, overlay::Overlay};
//! use poirot::model::{CellValue, Workbook};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_formula("B1".parse()?, "A1*2")?;
//! let mut overlay = Overlay::with_workbook(book).build()?;
//! let input = overlay.target("Data!A1")?;
//! let output = overlay.target("Data!B1")?;
//! overlay.set_value(input, 4.)?;
//! let context = EvaluationContext::default();
//! {
//!     let mut cycle = overlay.calculation(&context)?;
//!     assert_eq!(cycle.cell(output)?, CellValue::Number(8.));
//! }
//! overlay.apply_edits_to_workbook()?;
//! let book = overlay.into_workbook();
//! assert_eq!(book.sheet(0).and_then(|sheet| sheet.get("A1".parse().ok()?))
//!     .map(|cell| &cell.value), Some(&CellValue::Number(4.)));
//! # Ok(())
//! # }
//! ```
//!
//! Depends on: session (sessions and cell evaluation), run (evaluation runs and
//! reports), compile (compiled plans), config (evaluation context, policy and run
//! configuration), functions (extensions), coordinates (targets and coordinate tables),
//! limits (budgets and failures), values (CellEvaluation outputs), spill (retained
//! spill ownership), materialization (formula cache publication), reuse (retained
//! results and dependency evidence), ports (consumed-read events) and inputs (sparse
//! edits and effective workbook).

/// Borrowed runtime sources captured by a snapshot.
mod borrowed_source;
/// Serialized retained result with its consumed dependencies.
mod cached_value;
/// Ordinary direct calculation without mandatory detailed profiling.
mod calculation;
/// Comparison of stored cells and collection of cell and result changes.
mod changes;
/// Cumulative compilation work, independent of formula execution counters.
mod compilation_usage;
/// Persistent execution owner reborrowed by temporary source and observer ports.
mod continuation;
/// Content or style intent of an authored edit.
mod edit_kind;
/// Direct scenario queries that keep the baseline workbook unchanged.
mod evaluation;
/// Configuration of a direct evaluation.
mod evaluation_builder;
/// One ordinary overlay calculation with persistent results.
mod evaluation_cycle;
/// Calculated value with optional detailed profiling.
mod evaluation_report;
/// Editable workbook scenarios that reuse calculations across input changes.
mod incremental;
/// Sheet-qualified local cell reference parsing.
mod local_cell_target;
/// Shared output selection over the single execution engine.
mod output;
/// Configures an overlay's workbook, formula policies, and custom functions.
mod overlay_builder;
/// Batched authored edits that invalidate an overlay's retained results once.
mod overlay_edits;
/// Failure to construct, restore, or operate on an overlay.
mod overlay_error;
/// Bounded execution borrowing an overlay's retained state.
mod overlay_evaluation;
/// Ownership of the runtime extension registry attached to a source.
mod registry_handle;
/// Serializable views of workbook, edits, compiled formulas, and cached results.
pub mod snapshot;
/// Where restoration finds the workbook that prepared state was built against.
mod snapshot_baseline;
/// Versioned wire payload of an overlay snapshot.
mod snapshot_payload;
/// Borrowed or owned state behind an overlay snapshot.
mod snapshot_state;
/// Shared sparse source, parsing policies, and lazy compilation.
mod source;
/// Whether an authored mutation changed only literal values.
mod source_change;
/// Unit tests for incremental overlay reuse, edits and flushing.
#[cfg(test)]
mod tests;
/// Detached overlay source and retained results awaiting a baseline workbook.
mod workspace_state;

// Editable workspaces: direct evaluations and retained incremental overlays.
pub use evaluation::Evaluation;
pub use evaluation_builder::EvaluationBuilder;
pub use incremental::Overlay;
pub use overlay_builder::OverlayBuilder;
pub use overlay_edits::OverlayEdits;
pub use overlay_error::OverlayError;
pub use overlay_evaluation::OverlayEvaluation;
pub use workspace_state::WorkspaceState;

// Ordinary calculation reports distinguish disabled profiling from zero work.
pub use calculation::Calculation;
pub use compilation_usage::CompilationUsage;
pub use evaluation_cycle::EvaluationCycle;
pub use evaluation_report::EvaluationReport;
