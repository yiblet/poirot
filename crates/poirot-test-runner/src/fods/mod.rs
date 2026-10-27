//! Read original FODS fixtures and evaluate their upstream checks in Rust.
mod execution_trace;
mod inspection_location;
mod name_admission;
mod name_admission_error;
mod workbook_preparation_error;

/// Stable source-case identity and coordinate helpers.
pub mod cases;
/// Typed command-line contracts for corpus operations.
pub mod cli;
/// Command execution for running, inspecting, refreshing, and classifying fixtures.
pub mod commands;
/// Pinned corpus metadata and integrity-checked native fixture loading.
pub mod corpus;
/// Parallel execution of prepared fixture plans.
pub mod execute;
/// Infrastructure diagnostic channel used by runner commands.
mod fods_error;
/// ODF-to-supported-formula translation.
pub mod formula;
/// Typed classification of semantic, capability, and harness differences.
pub mod gaps;
/// Pure fixture preparation before parallel evaluation.
pub mod plan;
/// Typed case outcomes and deterministic report rendering.
pub mod report;
/// Native source extraction and retained fixture provenance.
pub mod xml;

/// Runner infrastructure failure type shared by command entry points.
pub use fods_error::FodsError;
