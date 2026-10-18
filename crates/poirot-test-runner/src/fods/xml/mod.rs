//! Typed adaptation of the pinned original LibreOffice FODS fixtures.
/// Typed extracted source records shared by corpus refresh and execution planning.
pub mod model;
mod reader;

/// Pinned LibreOffice revision from which the native fixtures were taken.
pub const SOURCE_COMMIT: &str = "cd35bc0879ed5e4b8138b724033ea5937751a84d";
/// ODF formula spelling translation into the evaluator's supported syntax.
pub use crate::fods::formula::translate;
/// Native FODS extraction retaining source cases and fixture provenance.
pub use reader::{extract_bytes, fixture_data};
