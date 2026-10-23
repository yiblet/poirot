//! Workbook-owned entry points for scenarios and effective-state export.
use crate::{EvaluationContext, OverlayError, Workbook, WorkbookError, eval, io, model};
use std::{
    borrow::Borrow,
    ops::{Deref, DerefMut},
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
pub struct WorkbookEvaluationBuilder<'a> {
    workbook: &'a Workbook,
    context: Option<EvaluationContext>,
    policy: EvaluationPolicy,
    profiling: bool,
    limits: Option<eval::EvaluationLimits>,
}
impl<'a> WorkbookEvaluationBuilder<'a> {
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
    pub fn build(self) -> Result<WorkbookEvaluation<'a>, OverlayError> {
        let mut context = self.context.unwrap_or_default();
        if let Some(limits) = self.limits {
            context = context.limits(limits);
        }
        Ok(WorkbookEvaluation {
            inner: eval::overlay::Evaluation::with_workbook(self.workbook)
                .with_policy(self.policy)
                .build_calculation_with_profiling(context, self.profiling)?,
        })
    }
}

/// Direct selected-output calculation with workbook file export.
/// Typed targets remain bound to the source workbook's sheet topology, not arbitrary structural edits.
pub struct WorkbookEvaluation<'a, W = &'a Workbook> {
    inner: eval::overlay::Calculation<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> WorkbookEvaluation<'a, W> {
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
    pub fn finish(self) -> eval::overlay::CalculationReport<()> {
        self.inner.finish()
    }
    /// Take the existing calculation without copying source or losing its state.
    pub fn into_calculation(self) -> eval::overlay::Calculation<'a, W> {
        self.inner
    }
}
impl<'a, W> Deref for WorkbookEvaluation<'a, W> {
    /// Existing direct calculation operations.
    type Target = eval::overlay::Calculation<'a, W>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<W> DerefMut for WorkbookEvaluation<'_, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Configure incremental scenarios; build never evaluates cells or clones the baseline.
pub struct WorkbookOverlayBuilder<'a> {
    workbook: &'a Workbook,
    policy: EvaluationPolicy,
}
impl<'a> WorkbookOverlayBuilder<'a> {
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
    pub fn build(self) -> Result<WorkbookOverlay<'a>, OverlayError> {
        Ok(WorkbookOverlay {
            inner: eval::overlay::Overlay::with_workbook(self.workbook)
                .with_policy(self.policy)
                .build()?,
        })
    }
}
/// Incremental scenarios retaining unaffected results, with direct effective XLSX export.
/// Use `calculation(&context)` to start an ordinary explicit cycle.
pub struct WorkbookOverlay<'a, W = &'a Workbook> {
    inner: eval::overlay::Overlay<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> WorkbookOverlay<'a, W> {
    /// Add file export to an existing borrowed, owned, or shared overlay without copying it.
    pub fn from_overlay(inner: eval::overlay::Overlay<'a, W>) -> Self {
        Self { inner }
    }
    /// Start an ordinary explicit cycle; valid cached results survive and work resets.
    pub fn evaluation<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
    ) -> Result<eval::overlay::CalculationCycle<'e>, OverlayError> {
        self.inner.calculation(context)
    }
    /// Start an explicit cycle with optional detailed profiling independent of quotas.
    pub fn evaluation_with_profiling<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
        profiling: bool,
    ) -> Result<eval::overlay::CalculationCycle<'e>, OverlayError> {
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
impl<'a, W> Deref for WorkbookOverlay<'a, W> {
    /// Existing advanced incremental operations.
    type Target = eval::overlay::Overlay<'a, W>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<W> DerefMut for WorkbookOverlay<'_, W> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
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
    ) -> Result<RestoredWorkbookEvaluation, eval::artifact::SnapshotError> {
        self.into_evaluation_with_policy(context, EvaluationPolicy::default())
    }
    /// Set fallback semantics only when preparation is absent or discarded.
    /// Compatible saved policies are preserved, rather than replaced by this fallback.
    pub fn into_evaluation_with_policy(
        self,
        context: EvaluationContext,
        recovery_policy: EvaluationPolicy,
    ) -> Result<RestoredWorkbookEvaluation, eval::artifact::SnapshotError> {
        let restored = self.inner.into_evaluation(context, recovery_policy)?;
        Ok(RestoredWorkbookEvaluation {
            evaluation: WorkbookEvaluation {
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
pub struct RestoredWorkbookEvaluation {
    /// Ordinary direct execution, with effective file export.
    pub evaluation: WorkbookEvaluation<'static, model::Workbook>,
    /// Whether optional preparation was reused, absent, or discarded.
    pub prepared_status: eval::artifact::PreparedStatus,
}
