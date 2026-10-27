//! Cells and names currently under evaluation within a run.

use crate::eval::coordinates::{CellSet, EvaluationTarget};
use std::collections::HashSet;

/// Targets and defined names currently being evaluated: detects cell and name
/// cycles and attributes reads and charges to the innermost consuming target.
#[derive(Default)]
pub(in crate::eval) struct EvaluationStack {
    /// Innermost target whose evaluation consumes the current reads.
    pub(in crate::eval) consumer: Option<EvaluationTarget>,
    /// Cells whose evaluation is in progress.
    pub(in crate::eval) visiting: CellSet,
    /// Defined-name records, keyed by caller and name index, whose resolution is in progress.
    pub(in crate::eval) names: HashSet<(usize, usize)>,
}
