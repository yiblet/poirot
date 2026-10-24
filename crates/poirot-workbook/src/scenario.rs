//! Workbook-owned entry points for scenarios and effective-state export.
use crate::{EvaluationContext, OverlayError, Workbook, WorkbookError, eval, io, model};
use crate::{EvaluationCycle, EvaluationFailure, EvaluationReport, EvaluationTarget};
use model::{CellValue, ModelError, WorkbookChange};
use std::{
    borrow::{Borrow, BorrowMut},
    path::Path,
};

/// Shared spreadsheet semantics for direct and incremental evaluation.
/// The compiler-only policy remains `eval::EvaluationPolicy` (`PreparationPolicy`).
pub type EvaluationPolicy = eval::overlay::OverlayPolicy;
/// Formula grammar and preparation policy, distinct from calculation semantics.
pub type PreparationPolicy = eval::EvaluationPolicy;
/// Input-number parsing locale; compatibility spelling is `NumberLocaleV2`.
pub type NumberLocale = eval::NumberLocaleV2;

/// Configure a borrowed direct evaluation without copying workbook data.
/// Complete context replacement precedes any subsequent individual limits override.
pub struct EvaluationBuilder<'a> {
    workbook: &'a Workbook,
    context: Option<EvaluationContext>,
    policy: EvaluationPolicy,
    profiling: bool,
    limits: Option<eval::EvaluationLimits>,
}
impl<'a> EvaluationBuilder<'a> {
    /// Configure an ordinary calculation over a borrowed public workbook.
    pub(crate) fn new(workbook: &'a Workbook) -> Self {
        Self {
            workbook,
            context: None,
            policy: EvaluationPolicy::default(),
            profiling: false,
            limits: None,
        }
    }
    /// Replace clock, seed, locale and limits; captured once when built if omitted.
    pub fn context(mut self, context: EvaluationContext) -> Self {
        self.context = Some(context);
        self.limits = None;
        self
    }
    /// Replace spreadsheet semantics for this evaluation.
    pub fn policy(mut self, policy: EvaluationPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Replace only evaluation quotas, preserving the other context settings.
    pub fn limits(mut self, limits: eval::EvaluationLimits) -> Self {
        self.limits = Some(limits);
        self
    }
    /// Enable or disable detailed profiling, independently of limits.
    pub fn profiling(mut self, enabled: bool) -> Self {
        self.profiling = enabled;
        self
    }
    /// Capture context once and create a direct calculation without evaluating any cells.
    pub fn build(self) -> Result<Evaluation<'a>, OverlayError> {
        let mut context = self.context.unwrap_or_default();
        if let Some(limits) = self.limits {
            context = context.limits(limits);
        }
        Ok(Evaluation {
            inner: eval::overlay::Evaluation::with_workbook(self.workbook)
                .with_policy(self.policy)
                .build_calculation_with_profiling(context, self.profiling)?,
        })
    }
}

/// Direct selected-output calculation with workbook file export.
/// Typed targets remain bound to the source workbook's sheet topology, not arbitrary structural edits.
pub struct Evaluation<'a, W = &'a Workbook> {
    inner: eval::overlay::Calculation<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> Evaluation<'a, W> {
    /// Resolve a reusable address against the source workbook.
    pub fn target(&self, reference: &str) -> Result<EvaluationTarget, ModelError> {
        self.inner.target(reference)
    }
    /// Inspect the immutable baseline before authored edits.
    pub fn workbook(&self) -> &model::Workbook {
        self.inner.workbook()
    }
    /// Inspect the spreadsheet semantics for this scenario.
    pub fn policy(&self) -> &EvaluationPolicy {
        self.inner.policy()
    }
    /// Discard pending edits and calculated results, preserving the baseline.
    pub fn discard_edits(&mut self) {
        self.inner.discard_edits()
    }
    /// Clear calculated values while preserving authored edits.
    pub fn clear_calculated_cache(&mut self) {
        self.inner.clear_calculated_cache()
    }
    /// Replace an authored input and invalidate calculated results.
    pub fn set_value(
        &mut self,
        target: EvaluationTarget,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError> {
        self.inner.set_value(target, value)
    }
    /// Replace an authored formula and invalidate calculated results.
    pub fn set_formula(
        &mut self,
        target: EvaluationTarget,
        formula: impl Into<String>,
    ) -> Result<(), ModelError> {
        self.inner.set_formula(target, formula)
    }
    /// Calculate one cell and its dependencies in this cycle.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationFailure> {
        self.inner.cell(target)
    }
    /// Calculate selected outputs in caller order within this cycle.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.inner.outputs(targets)
    }
    /// Calculate all effective stored cells and produced spills.
    pub fn all(&mut self) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.inner.all()
    }
    /// Start a new cycle, retaining edits and resetting results, work and failure.
    pub fn start_cycle(&mut self, context: EvaluationContext) -> Result<(), OverlayError> {
        self.inner.start_cycle(context)
    }
    /// Inspect this cycle's captured context and quotas.
    pub fn context(&self) -> &EvaluationContext {
        self.inner.context()
    }
    /// Inspect completed values without calculating.
    pub fn values(&self) -> impl Iterator<Item = (EvaluationTarget, &CellValue)> {
        self.inner.values()
    }
    /// Inspect retained memo storage independently of profiling.
    pub fn retained_usage(&self) -> eval::run::MemoRetentionUsage {
        self.inner.retained_usage()
    }
    /// Count successful results retained in this cycle.
    pub fn cached_cells(&self) -> usize {
        self.inner.cached_cells()
    }
    /// Collect authored edits and valid calculated results without calculation.
    pub fn changes(&self) -> Vec<WorkbookChange> {
        self.inner.changes()
    }
    /// Consume this scenario and collect its effective changes.
    pub fn into_changes(self) -> Vec<WorkbookChange> {
        self.inner.into_changes()
    }
    /// Configure native persistence without calculating or ending the cycle.
    pub fn save_snapshot(&self) -> eval::artifact::SnapshotSaveBuilder<'_> {
        self.inner.save_snapshot()
    }

    /// Add file export to an existing borrowed, owned, or shared calculation without copying it.
    pub fn from_calculation(inner: eval::overlay::Calculation<'a, W>) -> Self {
        Self { inner }
    }
    /// Capture effective authored edits and valid results now, without calculating or changing state.
    /// Later edits do not change the returned builder's captured workbook.
    /// Untouched saved formula results are preserved and may be stale.
    pub fn save_xlsx(&self) -> ScenarioXlsxSaveBuilder {
        ScenarioXlsxSaveBuilder::new(self.inner.workbook(), self.inner.changes())
    }
    /// Finish this cycle with optional profiling information, without applying authored edits.
    pub fn finish(self) -> EvaluationReport<()> {
        self.inner.finish()
    }
    /// Take the existing calculation without copying source or losing its state.
    pub fn into_calculation(self) -> eval::overlay::Calculation<'a, W> {
        self.inner
    }
}

/// Configure incremental scenarios; build never evaluates cells or clones the baseline.
pub struct OverlayBuilder<'a> {
    workbook: &'a Workbook,
    policy: EvaluationPolicy,
}
impl<'a> OverlayBuilder<'a> {
    /// Configure an incremental workspace over a borrowed public workbook.
    pub(crate) fn new(workbook: &'a Workbook) -> Self {
        Self {
            workbook,
            policy: EvaluationPolicy::default(),
        }
    }
    /// Replace spreadsheet semantics shared with direct evaluation.
    pub fn policy(mut self, policy: EvaluationPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Create incremental state; select context and profiling explicitly for each cycle.
    pub fn build(self) -> Result<Overlay<'a>, OverlayError> {
        Ok(Overlay {
            inner: eval::overlay::Overlay::with_workbook(self.workbook)
                .with_policy(self.policy)
                .build()?,
        })
    }
}
/// Incremental scenarios retaining unaffected results, with direct effective XLSX export.
/// Use `evaluation(&context)` to start an ordinary explicit cycle.
pub struct Overlay<'a, W = &'a Workbook> {
    inner: eval::overlay::Overlay<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> Overlay<'a, W> {
    /// Resolve a reusable address against the source workbook.
    pub fn target(&self, reference: &str) -> Result<EvaluationTarget, ModelError> {
        self.inner.target(reference)
    }
    /// Inspect the immutable baseline before authored edits.
    pub fn workbook(&self) -> &model::Workbook {
        self.inner.workbook()
    }
    /// Inspect the spreadsheet semantics for this scenario.
    pub fn policy(&self) -> &EvaluationPolicy {
        self.inner.policy()
    }
    /// Discard pending edits and calculated results, preserving the baseline.
    pub fn discard_edits(&mut self) {
        self.inner.discard_edits()
    }
    /// Clear calculated values while preserving authored edits.
    pub fn clear_calculated_cache(&mut self) {
        self.inner.clear_calculated_cache()
    }
    /// Replace an authored input and invalidate calculated results.
    pub fn set_value(
        &mut self,
        target: EvaluationTarget,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError> {
        self.inner.set_value(target, value)
    }
    /// Replace an authored formula and invalidate calculated results.
    pub fn set_formula(
        &mut self,
        target: EvaluationTarget,
        formula: impl Into<String>,
    ) -> Result<(), ModelError> {
        self.inner.set_formula(target, formula)
    }
    /// Apply a batch of authored edits with one dependency invalidation.
    pub fn edit<R>(
        &mut self,
        apply: impl FnOnce(&mut eval::overlay::OverlayEdits<'_, 'a, W>) -> R,
    ) -> R {
        self.inner.edit(apply)
    }
    /// Inspect effective authored content before calculation.
    pub fn source_cell(&self, target: EvaluationTarget) -> Option<&model::Cell> {
        self.inner.source_cell(target)
    }
    /// Count authored overrides and removals.
    pub fn edited_cells(&self) -> usize {
        self.inner.edited_cells()
    }
    /// Replace authored style and invalidate affected results.
    pub fn set_style(
        &mut self,
        target: EvaluationTarget,
        style: model::CellStyle,
    ) -> Result<(), ModelError> {
        self.inner.set_style(target, style)
    }
    /// Change a formula's saved value; computing policies still recalculate it.
    pub fn set_cached_value(
        &mut self,
        target: EvaluationTarget,
        value: impl Into<CellValue>,
    ) -> Result<(), ModelError> {
        self.inner.set_cached_value(target, value)
    }
    /// Remove effective authored content and invalidate affected results.
    pub fn remove_cell(&mut self, target: EvaluationTarget) -> Result<(), ModelError> {
        self.inner.remove_cell(target)
    }
    /// Replace scenario semantics, invalidating affected preparation and results.
    pub fn set_policy(&mut self, policy: EvaluationPolicy) -> Result<(), OverlayError> {
        self.inner.set_policy(policy)
    }
    /// Report cumulative formula compilation and reuse.
    pub fn compiled_usage(&mut self) -> crate::CompilationUsage {
        self.inner.compiled_usage()
    }
    /// Prepare all effective formulas without executing them.
    pub fn compile_all(&mut self) -> crate::CompilationUsage {
        self.inner.compile_all()
    }
    /// Inspect retained memo storage independently of profiling.
    pub fn retained_usage(&mut self) -> eval::run::MemoRetentionUsage {
        self.inner.retained_usage()
    }
    /// Count current valid formula results and spill children.
    pub fn cached_cells(&mut self) -> usize {
        self.inner.cached_cells()
    }
    /// Inspect a valid cached result without calculating.
    pub fn cached_value(&self, target: EvaluationTarget) -> Option<std::sync::Arc<CellValue>> {
        self.inner.cached_value(target)
    }
    /// Collect authored edits and valid calculated results without calculation.
    pub fn changes(&mut self) -> Vec<WorkbookChange> {
        self.inner.changes()
    }
    /// Consume this scenario and collect its effective changes.
    pub fn into_changes(self) -> Vec<WorkbookChange> {
        self.inner.into_changes()
    }
    /// Configure native persistence without calculating or clearing warm state.
    pub fn save_snapshot(&self) -> eval::artifact::SnapshotSaveBuilder<'_> {
        self.inner.save_snapshot()
    }
    /// Borrow source, edits and reusable results for native serialization.
    pub fn snapshot(&self) -> eval::overlay::snapshot::Snapshot<'_> {
        self.inner.snapshot()
    }

    /// Add file export to an existing borrowed, owned, or shared overlay without copying it.
    pub fn from_overlay(inner: eval::overlay::Overlay<'a, W>) -> Self {
        Self { inner }
    }
    /// Start an ordinary explicit cycle; valid cached results survive and work resets.
    pub fn evaluation<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
    ) -> Result<EvaluationCycle<'e>, OverlayError> {
        self.inner.calculation(context)
    }
    /// Start an explicit cycle with optional detailed profiling independent of quotas.
    pub fn evaluation_with_profiling<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
        profiling: bool,
    ) -> Result<EvaluationCycle<'e>, OverlayError> {
        self.inner.calculation_with_profiling(context, profiling)
    }
    /// Capture effective authored edits and valid results now, without calculation or cache clearing.
    /// Later edits do not change the returned builder's captured workbook.
    /// Untouched saved formula results remain saved values, potentially stale.
    pub fn save_xlsx(&mut self) -> ScenarioXlsxSaveBuilder {
        let changes = self.inner.changes();
        ScenarioXlsxSaveBuilder::new(self.inner.workbook(), changes)
    }
    /// Take the existing overlay without copying source or clearing warm state.
    pub fn into_overlay(self) -> eval::overlay::Overlay<'a, W> {
        self.inner
    }
}

impl<W: Borrow<model::Workbook> + BorrowMut<model::Workbook>> Overlay<'_, W> {
    /// Apply authored edits atomically to an exclusively owned baseline, retaining valid results.
    pub fn apply_edits_to_workbook(&mut self) -> Result<usize, ModelError> {
        self.inner.apply_edits_to_workbook()
    }
}

/// Failure assembling effective scenario data or exporting its XLSX file.
#[derive(Debug)]
pub enum ScenarioSaveError {
    /// Effective edits could not be applied to an isolated copy.
    Model(model::ModelError),
    /// File serialization, export validation, or destination writing failed.
    Workbook(WorkbookError),
}
impl std::fmt::Display for ScenarioSaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(e) => write!(f, "assembling scenario export: {e}"),
            Self::Workbook(e) => write!(f, "saving scenario XLSX: {e}"),
        }
    }
}
impl std::error::Error for ScenarioSaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Model(e) => e,
            Self::Workbook(e) => e,
        })
    }
}
/// Owned effective-state export staged with one workbook copy, leaving scenario state untouched.
/// Serialization validates once and never evaluates formulas. Writing is not an edit commit.
#[must_use]
pub struct ScenarioXlsxSaveBuilder {
    workbook: Result<model::Workbook, model::ModelError>,
    options: io::WriteOptions,
}
impl ScenarioXlsxSaveBuilder {
    fn new(workbook: &model::Workbook, changes: Vec<model::WorkbookChange>) -> Self {
        Self {
            workbook: workbook.with_changes(changes),
            options: io::WriteOptions::default(),
        }
    }
    /// Replace export options; defaults match `Workbook::save_xlsx`.
    pub fn options(mut self, options: io::WriteOptions) -> Self {
        self.options = options;
        self
    }
    /// Validate and serialize the effective workbook without running formulas.
    pub fn to_buffer(self) -> Result<Vec<u8>, ScenarioSaveError> {
        let workbook = self.workbook.map_err(ScenarioSaveError::Model)?;
        io::plan_xlsx_export(&workbook, self.options)
            .and_then(|plan| plan.execute())
            .map_err(|e| ScenarioSaveError::Workbook(e.into()))
    }
    /// Serialize before opening the destination; filesystem failure may leave a partial file.
    pub fn write_file(self, path: impl AsRef<Path>) -> Result<(), ScenarioSaveError> {
        let bytes = self.to_buffer()?;
        std::fs::write(path, bytes).map_err(|e| ScenarioSaveError::Workbook(WorkbookError::File(e)))
    }
}

/// Snapshot restoration with default source recovery and explicit preparation disposition.
/// `Workbook::load_snapshot` retains the original advanced artifact loader.
#[derive(Default)]
pub struct WorkbookSnapshotLoadBuilder {
    inner: eval::artifact::SnapshotLoadBuilder,
    require_prepared: bool,
}
impl WorkbookSnapshotLoadBuilder {
    /// Replace snapshot input quotas without changing export defaults.
    pub fn limits(mut self, limits: eval::artifact::SnapshotLimits) -> Self {
        self.inner = self.inner.limits(limits);
        self
    }
    /// Choose compatible reuse, strict preparation (also refusing absent state), or source-only recovery.
    /// Unlike the advanced artifact loader's compatibility-if-present policy, strict mode here
    /// requires preparation to be present as well as compatible.
    pub fn prepared_policy(mut self, policy: eval::artifact::PreparedPolicy) -> Self {
        self.require_prepared = policy == eval::artifact::PreparedPolicy::RequireCompatible;
        self.inner = self.inner.prepared_policy(policy);
        self
    }
    /// Decode native state into a usable restoration object, keeping preparation status visible.
    pub fn from_bytes(
        self,
        bytes: &[u8],
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        Self::validated(self.inner.from_bytes(bytes)?, self.require_prepared)
    }
    /// Read native state without evaluating formulas.
    pub fn open(
        self,
        path: impl AsRef<Path>,
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        Self::validated(self.inner.open(path)?, self.require_prepared)
    }
    fn validated(
        inner: eval::artifact::RestoredArtifact,
        require_prepared: bool,
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        if require_prepared && inner.prepared_status != eval::artifact::PreparedStatus::Restored {
            return Err(eval::artifact::SnapshotError::Envelope(
                "compatible prepared state required but absent",
            ));
        }
        Ok(RestoredWorkbook { inner })
    }
}
/// Validated snapshot source and optional preparation, ready to become a calculation.
pub struct RestoredWorkbook {
    inner: eval::artifact::RestoredArtifact,
}
impl RestoredWorkbook {
    /// Inspect whether optional preparation survived decoding.
    pub fn prepared_status(&self) -> eval::artifact::PreparedStatus {
        self.inner.prepared_status
    }
    /// Restore direct calculation; compatible saved semantics win over fallback defaults.
    /// Source-only recovery uses default semantics. The supplied context starts one new cycle.
    pub fn into_evaluation(
        self,
        context: EvaluationContext,
    ) -> Result<RestoredEvaluation, eval::artifact::SnapshotError> {
        self.into_evaluation_with_policy(context, EvaluationPolicy::default())
    }
    /// Set fallback semantics only when preparation is absent or discarded.
    /// Compatible saved policies are preserved, rather than replaced by this fallback.
    pub fn into_evaluation_with_policy(
        self,
        context: EvaluationContext,
        recovery_policy: EvaluationPolicy,
    ) -> Result<RestoredEvaluation, eval::artifact::SnapshotError> {
        let restored = self.inner.into_evaluation(context, recovery_policy)?;
        Ok(RestoredEvaluation {
            evaluation: Evaluation {
                inner: restored.evaluation.into_calculation(false),
            },
            prepared_status: restored.prepared_status,
        })
    }
    /// Recover the advanced artifact API without copying source or prepared state.
    pub fn into_artifact(self) -> eval::artifact::RestoredArtifact {
        self.inner
    }
}
/// A usable owned evaluation together with its explicit preparation disposition.
pub struct RestoredEvaluation {
    /// Ordinary direct execution, with effective file export.
    pub evaluation: Evaluation<'static, model::Workbook>,
    /// Whether optional preparation was reused, absent, or discarded.
    pub prepared_status: eval::artifact::PreparedStatus,
}
