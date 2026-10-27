//! Ordinary calculation without mandatory detailed profiling.
use super::evaluation::Evaluation;
use super::evaluation_report::EvaluationReport;
use super::overlay_error::OverlayError;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationFailure;
use crate::model::{CellValue, ModelError, Workbook, WorkbookChange};
use std::borrow::Borrow;

/// Direct scenarios with opt-in profiling and explicit cycle boundaries.
/// Edits persist; each edit clears calculated results. `begin` resets work and failure.
/// Use the legacy `Evaluation` builder when detailed reports are always required.
///
/// ```
/// use poirot::eval::{EvaluationContext, overlay::Evaluation};
/// use poirot::model::{CellValue, Workbook};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("A1".parse()?, "2*3")?;
/// let mut calculation = Evaluation::with_workbook(book)
///     .build_calculation(EvaluationContext::default())?;
/// let output = calculation.target("Data!A1")?;
/// assert_eq!(calculation.cell(output)?, CellValue::Number(6.));
/// let book = calculation.into_workbook();
/// assert_eq!(book.sheet(0).and_then(|sheet| sheet.get("A1".parse().ok()?))
///     .and_then(|cell| cell.formula.as_deref()), Some("2*3"));
/// # Ok(())
/// # }
/// ```
pub struct Calculation<'a, W = &'a Workbook> {
    pub(super) inner: Evaluation<'a, W>,
    pub(super) profiling: bool,
}

impl<'a, W: Borrow<Workbook>> Calculation<'a, W> {
    /// Resolve a reusable target against this workbook's sheet topology.
    pub fn target(&self, reference: &str) -> Result<EvaluationTarget, ModelError> {
        self.inner.target(reference)
    }

    /// Calculate a cell; all reads share this cycle's work allowance.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationFailure> {
        self.inner.cell(target)
    }

    /// Calculate selected outputs in caller order within this cycle.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationFailure> {
        self.inner.outputs(targets)
    }

    /// Explicitly calculate all effective stored cells and produced spills.
    pub fn all(&mut self) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationFailure> {
        self.inner.all()
    }

    /// Replace an authored input and invalidate this mode's calculated cache.
    pub fn set_value(
        &mut self,
        target: EvaluationTarget,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError> {
        self.inner.set_value(target, value)
    }

    /// Replace an authored formula, retaining edits across calculation cycles.
    pub fn set_formula(
        &mut self,
        target: EvaluationTarget,
        formula: impl Into<String>,
    ) -> Result<(), ModelError> {
        self.inner.set_formula(target, formula)
    }

    /// Clear calculated values while preserving edits, work, and any refusal.
    pub fn clear_cache(&mut self) {
        self.inner.clear_cache();
    }

    /// Start an explicit cycle, preserving edits and compilation while resetting results and failure.
    pub fn begin(&mut self, context: EvaluationContext) -> Result<(), OverlayError> {
        self.inner.begin(context)?;
        self.inner.set_profiling(self.profiling);
        Ok(())
    }

    /// Start a new cycle, retaining edits and clearing results, work, and failure.
    pub fn start_cycle(&mut self, context: EvaluationContext) -> Result<(), OverlayError> {
        self.begin(context)
    }

    /// Clear calculated values without changing edits, work, or failure.
    pub fn clear_calculated_cache(&mut self) {
        self.clear_cache();
    }

    /// Discard pending edits and calculated values without changing the baseline.
    pub fn discard_edits(&mut self) {
        self.inner.discard_edits();
    }

    /// Inspect the spreadsheet semantics shared with overlays.
    pub fn policy(&self) -> &EvaluationPolicy {
        self.inner.policy()
    }

    /// Inspect captured context and configured limits.
    pub fn context(&self) -> &EvaluationContext {
        self.inner.context()
    }

    /// Inspect completed local values without recalculating.
    pub fn values(&self) -> impl Iterator<Item = (EvaluationTarget, &CellValue)> {
        self.inner.values()
    }

    /// Inspect logical memo entries and bytes retained in this cycle, independently of profiling.
    pub fn retained_usage(&self) -> crate::eval::limits::MemoRetentionUsage {
        self.inner.retained_usage()
    }

    /// Count local results retained from successful output calls.
    pub fn cached_cells(&self) -> usize {
        self.inner.cached_cells()
    }

    /// Export authored edits and valid calculated results without evaluation.
    pub fn changes(&self) -> Vec<WorkbookChange> {
        self.inner.changes()
    }

    /// Consume the workspace and export authored edits and valid calculated results.
    pub fn into_changes(self) -> Vec<WorkbookChange> {
        self.inner.into_changes()
    }

    /// Borrow the immutable source baseline before authored edits.
    pub fn workbook(&self) -> &Workbook {
        self.inner.workbook()
    }

    /// Recover the original source handle, discarding scenario edits and calculated results.
    pub fn into_workbook(self) -> W {
        self.inner.into_workbook()
    }

    /// Borrow serializable source and bytecode without calculating or ending this cycle.
    pub fn snapshot(&self) -> crate::eval::overlay::snapshot::Snapshot<'_> {
        self.inner.snapshot()
    }

    /// Finish this cycle with honestly optional profiling information.
    pub fn finish(self) -> EvaluationReport<()> {
        EvaluationReport::from_legacy(self.inner.finish(), self.profiling)
    }
}
