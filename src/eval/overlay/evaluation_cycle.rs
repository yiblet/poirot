//! One ordinary calculation borrowing an overlay's retained state.

use super::evaluation_report::EvaluationReport;
use super::overlay_evaluation::OverlayEvaluation;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationError;
use crate::model::{CellValue, WorkbookChange};

/// An overlay cycle whose valid results persist, with optional detailed profiling.
/// Dropping this cycle preserves successful cache publication and authored edits.
///
/// ```
/// use poirot::eval::{EvaluationContext, overlay::Overlay};
/// use poirot::model::{CellValue, Workbook};
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1", "2*3")?;
/// let mut scenario = Overlay::with_workbook(&book).build()?;
/// let output = scenario.target("Data!A1")?;
/// let context = EvaluationContext::default();
/// let mut cycle = scenario.calculation(&context)?;
/// assert_eq!(cycle.cell(output)?, CellValue::Number(6.));
/// cycle.finish().result?;
/// assert_eq!(scenario.cached_cells(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct EvaluationCycle<'a> {
    pub(super) inner: OverlayEvaluation<'a>,
    pub(super) profiling: bool,
}

impl EvaluationCycle<'_> {
    /// Calculate one cell using this cycle's remaining work and the overlay's valid cache.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationError> {
        self.inner.cell(target)
    }

    /// Calculate selected outputs atomically within the same work allowance.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationError> {
        self.inner.outputs(targets)
    }

    /// Explicitly calculate all effective stored cells and spill children.
    pub fn all(&mut self) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationError> {
        self.inner.all()
    }

    /// Export this cycle's completed result changes without calculating.
    pub fn result_changes(&self) -> Result<Vec<WorkbookChange>, EvaluationError> {
        self.inner.result_changes()
    }

    /// Finish this cycle; unavailable profiling is represented by `None`.
    pub fn finish(self) -> EvaluationReport<()> {
        EvaluationReport::from_legacy(self.inner.finish(), self.profiling)
    }
}
