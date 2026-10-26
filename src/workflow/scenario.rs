//! Workbook-owned entry points for scenarios and effective-state export.
use crate::workflow::{EvaluationContext, OverlayError, Workbook, WorkbookError, eval, io, model};
use crate::workflow::{EvaluationCycle, EvaluationFailure, EvaluationReport, EvaluationTarget};
use model::{CellValue, ModelError, WorkbookChange};
use std::{
    borrow::{Borrow, BorrowMut},
    path::Path,
};

pub use eval::overlay::EvaluationPolicy;
pub use eval::{NumberLocale, PreparationPolicy};

/// Configure a direct evaluation over a borrowed or owned source without copying it.
///
/// [`Self::build`] captures one context and starts a cycle without evaluating cells.
/// Complete context replacement precedes any subsequent individual limits override.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1".parse()?, "A1*3")?;
/// use poirot::eval::{EvaluationContext, EvaluationLimits};
/// let context = EvaluationContext { randomness_seed: 42, ..Default::default() };
/// let mut run = book.evaluation().context(context)
///     .limits(EvaluationLimits::default().max_work(10_000)).build()?;
/// let output = run.target("Data!B1")?;
/// run.set_value(run.target("Data!A1")?, 4.)?;
/// assert_eq!(run.cell(output)?, CellValue::Number(12.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct EvaluationBuilder<'a, W = &'a model::Workbook> {
    workbook: W,
    lifetime: std::marker::PhantomData<&'a ()>,
    context: Option<EvaluationContext>,
    policy: EvaluationPolicy,
    profiling: bool,
    limits: Option<eval::EvaluationLimits>,
}
impl<'a, W: Borrow<model::Workbook>> EvaluationBuilder<'a, W> {
    /// Configure an ordinary calculation over its original source handle.
    pub(in crate::workflow) fn new(workbook: W) -> Self {
        Self {
            workbook,
            lifetime: std::marker::PhantomData,
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
    pub fn build(self) -> Result<Evaluation<'a, W>, OverlayError> {
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

/// A direct calculation workspace with scenario inputs separate from its baseline.
///
/// Reads calculate only requested cells and dependencies. Results share this cycle's
/// context and work allowance; edits clear calculated results, and [`Self::start_cycle`]
/// also resets work and failure. Use [`Overlay`] to retain unaffected results across edits.
/// Neither calculation nor export mutates the baseline workbook.
///
/// [`Workbook::evaluation`] borrows its source; [`Workbook::into_evaluation`] moves
/// it without copying. Use [`Self::from_calculation`] for custom engine configuration
/// or shared ownership. Targets remain bound to that source's sheet topology and
/// may be invalidated by structural edits.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1".parse()?, "A1*3")?;
/// let mut run = book.into_evaluation().build()?;
/// let output = run.target("Data!B1")?;
/// run.set_value(run.target("Data!A1")?, 5.)?;
/// assert_eq!(run.cell(output)?, CellValue::Number(15.));
/// let captured = run.save_xlsx().to_buffer()?;
/// let original = run.into_workbook();
/// assert!(original.stored_cell("Data", "A1".parse()?).is_none());
/// let exported = Workbook::from_bytes(&captured)?;
/// assert_eq!(exported.stored_cell("Data", "B1".parse()?).ok_or("missing result")?.value,
///     CellValue::Number(15.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Evaluation<'a, W = &'a model::Workbook> {
    inner: eval::overlay::Calculation<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> Evaluation<'a, W> {
    /// Resolve a reusable sheet-qualified address, such as `Data!A1` or `'Annual summary'!B12`.
    ///
    /// Invalid references and missing sheets return an error. Unstored coordinates
    /// on an existing sheet are valid targets; they calculate as blank when no
    /// source, scenario edit or spill supplies a value. Resolution does not
    /// calculate; targets remain tied to the source sheet topology.
    ///
    /// ```
    /// use poirot::Workbook;
    /// use poirot::model::CellValue;
    /// let mut book = Workbook::new();
    /// book.add_sheet("Annual summary")?;
    /// let mut run = book.evaluation().build()?;
    /// let output = run.target("'Annual summary'!B12")?;
    /// assert_eq!(run.cell(output)?, CellValue::Blank);
    /// assert!(run.target("Missing!B12").is_err());
    /// assert!(run.target("'Annual summary'!A0").is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
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
    ) -> Result<Vec<crate::workflow::CellEvaluation>, EvaluationFailure> {
        self.inner.outputs(targets)
    }
    /// Calculate all effective stored cells and produced spills.
    pub fn all(&mut self) -> Result<Vec<crate::workflow::CellEvaluation>, EvaluationFailure> {
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

/// Configure an incremental workspace over a borrowed or owned baseline.
///
/// Building creates scenario state without evaluating cells or cloning the source.
/// Context and profiling are selected later for each [`Overlay::evaluation`] cycle.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1".parse()?, "A1*3")?;
/// use poirot::calculation::EvaluationPolicy;
/// let policy = EvaluationPolicy::default();
/// let mut scenario = book.into_overlay().policy(policy).build()?;
/// let input = scenario.target("Data!A1")?;
/// scenario.set_value(input, 4.)?;
/// assert_eq!(scenario.apply_edits_to_workbook()?, 1);
/// let book = scenario.into_workbook();
/// assert_eq!(book.stored_cell("Data", "A1".parse()?).ok_or("missing input")?.value,
///     CellValue::Number(4.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct OverlayBuilder<'a, W = &'a model::Workbook> {
    workbook: W,
    lifetime: std::marker::PhantomData<&'a ()>,
    policy: EvaluationPolicy,
}
impl<'a, W: Borrow<model::Workbook>> OverlayBuilder<'a, W> {
    /// Configure an incremental workspace over its original source handle.
    pub(in crate::workflow) fn new(workbook: W) -> Self {
        Self {
            workbook,
            lifetime: std::marker::PhantomData,
            policy: EvaluationPolicy::default(),
        }
    }
    /// Replace spreadsheet semantics shared with direct evaluation.
    pub fn policy(mut self, policy: EvaluationPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Create incremental state; select context and profiling explicitly for each cycle.
    pub fn build(self) -> Result<Overlay<'a, W>, OverlayError> {
        Ok(Overlay {
            inner: eval::overlay::Overlay::with_workbook(self.workbook)
                .with_policy(self.policy)
                .build()?,
        })
    }
}
/// A scenario workspace retaining dependency evidence and results across input edits.
///
/// Edits override the baseline without changing it. [`Self::evaluation`] borrows this
/// workspace for one cycle, resets its work allowance, and validates reusable results
/// as outputs are requested. Dirty results are not exposed as current cached values.
/// Export captures effective edits and valid results without running formulas.
///
/// [`Workbook::overlay`] borrows its source; [`Workbook::into_overlay`] moves it
/// without copying. Use [`Self::from_overlay`] for custom engine configuration or
/// shared ownership. Targets remain tied to the source sheet topology.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1".parse()?, "A1*3")?;
/// use poirot::calculation::EvaluationContext;
/// let mut scenario = book.into_overlay().build()?;
/// let input = scenario.target("Data!A1")?;
/// let output = scenario.target("Data!B1")?;
/// let context = EvaluationContext::default();
/// for value in [4., 5.] {
///     scenario.set_value(input, value)?;
///     assert_eq!(scenario.evaluation(&context)?.cell(output)?, CellValue::Number(value*3.));
/// }
/// scenario.apply_edits_to_workbook()?;
/// let mut book = scenario.into_workbook();
/// book.recalculate()?;
/// assert_eq!(book.stored_cell("Data", "B1".parse()?).ok_or("missing result")?.value,
///     CellValue::Number(15.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Overlay<'a, W = &'a model::Workbook> {
    inner: eval::overlay::Overlay<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> Overlay<'a, W> {
    /// Resolve a reusable sheet-qualified address, such as `Data!A1` or `'Annual summary'!B12`.
    ///
    /// Invalid references and missing sheets return an error. Unstored coordinates
    /// on an existing sheet are valid targets; they calculate as blank when no
    /// source, scenario edit or spill supplies a value. Resolution does not
    /// calculate; targets remain tied to the source sheet topology.
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
    pub fn compiled_usage(&mut self) -> crate::workflow::CompilationUsage {
        self.inner.compiled_usage()
    }
    /// Prepare all effective formulas without executing them.
    pub fn compile_all(&mut self) -> crate::workflow::CompilationUsage {
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

impl Evaluation<'_, model::Workbook> {
    /// Recover the baseline, discarding unapplied edits and calculated results.
    pub fn into_workbook(self) -> Workbook {
        Workbook::from_model(self.inner.into_workbook())
    }
}

impl Overlay<'_, model::Workbook> {
    /// Recover the baseline; explicitly apply authored edits first to preserve them.
    pub fn into_workbook(self) -> Workbook {
        Workbook::from_model(self.inner.into_workbook())
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
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1".parse()?, "A1*3")?;
/// let mut run = book.into_evaluation().build()?;
/// let input = run.target("Data!A1")?;
/// let output = run.target("Data!B1")?;
/// run.set_value(input, 4.)?;
/// run.cell(output)?;
/// let captured = run.save_xlsx();
/// run.set_value(input, 5.)?;
/// let exported = Workbook::from_bytes(&captured.to_buffer()?)?;
/// assert_eq!(exported.stored_cell("Data", "B1".parse()?).ok_or("missing result")?.value,
///     CellValue::Number(12.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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
///
/// ```
/// use poirot::Workbook;
/// use poirot::eval::artifact::{PreparedPolicy, PreparedStatus};
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1".parse()?, 42.)?;
/// let bytes = book.save_snapshot().to_buffer()?;
/// let restored = Workbook::restore_snapshot().from_bytes(&bytes)?;
/// assert_eq!(restored.prepared_status(), PreparedStatus::Absent);
/// assert!(Workbook::restore_snapshot().prepared_policy(PreparedPolicy::RequireCompatible)
///     .from_bytes(&bytes).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Default)]
pub struct RestoreBuilder {
    inner: eval::artifact::SnapshotLoadBuilder,
}
impl RestoreBuilder {
    /// Replace snapshot input quotas without changing export defaults.
    pub fn limits(mut self, limits: eval::artifact::SnapshotLimits) -> Self {
        self.inner = self.inner.limits(limits);
        self
    }
    /// Choose compatible reuse, strict preparation (also refusing absent state), or source-only recovery.
    pub fn prepared_policy(mut self, policy: eval::artifact::PreparedPolicy) -> Self {
        self.inner = self.inner.prepared_policy(policy);
        self
    }
    /// Decode native state into a usable restoration object, keeping preparation status visible.
    pub fn from_bytes(
        self,
        bytes: &[u8],
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        Ok(RestoredWorkbook {
            inner: self.inner.from_bytes(bytes)?,
        })
    }
    /// Read native state without evaluating formulas.
    pub fn open(
        self,
        path: impl AsRef<Path>,
    ) -> Result<RestoredWorkbook, eval::artifact::SnapshotError> {
        Ok(RestoredWorkbook {
            inner: self.inner.open(path)?,
        })
    }
}
/// Validated snapshot source and optional preparation, ready to become a calculation.
///
/// ```
/// use poirot::Workbook;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1".parse()?, 42.)?;
/// let bytes = book.save_snapshot().to_buffer()?;
/// let restored = Workbook::restore_snapshot().from_bytes(&bytes)?;
/// let run = restored.into_evaluation(Default::default())?;
/// let baseline = run.evaluation.into_workbook();
/// assert_eq!(baseline.stored_cell("Data", "A1".parse()?).ok_or("missing input")?.value,
///     poirot::model::CellValue::Number(42.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
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
///
/// ```
/// use poirot::Workbook;
/// use poirot::eval::artifact::PreparedStatus;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1".parse()?, 42.)?;
/// let bytes = book.save_snapshot().to_buffer()?;
/// let restored = Workbook::restore_snapshot().from_bytes(&bytes)?;
/// let mut run = restored.into_evaluation(Default::default())?;
/// assert_eq!(run.prepared_status, PreparedStatus::Absent);
/// let input = run.evaluation.target("Data!A1")?;
/// assert_eq!(run.evaluation.cell(input)?, poirot::model::CellValue::Number(42.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct RestoredEvaluation {
    /// Ordinary direct execution, with effective file export.
    pub evaluation: Evaluation<'static, model::Workbook>,
    /// Whether optional preparation was reused, absent, or discarded.
    pub prepared_status: eval::artifact::PreparedStatus,
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    /// Build a source whose arithmetic results follow directly from its two inputs.
    fn workbook() -> Workbook {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("Data").unwrap();
        sheet.set_value("A1".parse().unwrap(), 2.).unwrap();
        sheet.set_formula("B1".parse().unwrap(), "A1*3").unwrap();
        workbook
    }

    #[test]
    fn borrowed_builders_keep_the_original_model() {
        let workbook = workbook();
        let source = workbook.model() as *const model::Workbook;
        let evaluation = workbook.evaluation().build().unwrap();
        assert_eq!(evaluation.workbook() as *const model::Workbook, source);
        assert_eq!(evaluation.cached_cells(), 0);
        let overlay = workbook.overlay().build().unwrap();
        assert_eq!(overlay.workbook() as *const model::Workbook, source);
    }

    #[test]
    fn owned_direct_recovers_original_storage_without_scenario_edits() {
        let workbook = workbook();
        let sheets = workbook.sheets().as_ptr();
        let mut evaluation = workbook.into_evaluation().build().unwrap();
        assert_eq!(evaluation.workbook().sheets().as_ptr(), sheets);
        let input = evaluation.target("Data!A1").unwrap();
        let output = evaluation.target("Data!B1").unwrap();
        evaluation.set_value(input, 4.).unwrap();
        assert_eq!(evaluation.cell(output).unwrap(), CellValue::Number(12.));
        let recovered = evaluation.into_workbook();
        assert_eq!(recovered.sheets().as_ptr(), sheets);
        assert_eq!(
            recovered
                .stored_cell("Data", "A1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(2.)
        );
        assert_eq!(
            recovered
                .stored_cell("Data", "B1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Blank
        );
    }

    #[test]
    fn owned_overlay_flushes_only_authored_edits_and_remains_reusable() {
        let workbook = workbook();
        let sheets = workbook.sheets().as_ptr();
        let mut overlay = workbook.into_overlay().build().unwrap();
        assert_eq!(overlay.workbook().sheets().as_ptr(), sheets);
        let input = overlay.target("Data!A1").unwrap();
        let output = overlay.target("Data!B1").unwrap();
        overlay.set_value(input, 4.).unwrap();
        let context = EvaluationContext::default();
        assert_eq!(
            overlay.evaluation(&context).unwrap().cell(output).unwrap(),
            CellValue::Number(12.)
        );
        assert_eq!(overlay.apply_edits_to_workbook().unwrap(), 1);
        assert_eq!(overlay.edited_cells(), 0);
        assert_eq!(
            overlay
                .workbook()
                .sheet(0)
                .unwrap()
                .get("B1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Blank
        );
        assert_eq!(overlay.cached_cells(), 1);
        overlay.set_value(input, 5.).unwrap();
        assert_eq!(
            overlay.evaluation(&context).unwrap().cell(output).unwrap(),
            CellValue::Number(15.)
        );
        let recovered = overlay.into_workbook();
        assert_eq!(
            recovered
                .stored_cell("Data", "A1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(4.)
        );
    }

    #[test]
    fn owned_direct_failure_keeps_baseline_recoverable() {
        let workbook = workbook();
        let mut evaluation = workbook
            .into_evaluation()
            .limits(eval::EvaluationLimits::default().max_work(0))
            .build()
            .unwrap();
        let input = evaluation.target("Data!A1").unwrap();
        let output = evaluation.target("Data!B1").unwrap();
        evaluation.set_value(input, 9.).unwrap();
        assert!(evaluation.cell(output).is_err());
        let recovered = evaluation.into_workbook();
        assert_eq!(
            recovered
                .stored_cell("Data", "A1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(2.)
        );
        assert_eq!(
            recovered
                .stored_cell("Data", "B1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Blank
        );
    }

    #[test]
    fn owned_overlay_recovery_drops_unflushed_edits_without_copying() {
        let workbook = workbook();
        let sheets = workbook.sheets().as_ptr();
        let mut overlay = workbook.into_overlay().build().unwrap();
        overlay
            .set_value(overlay.target("Data!A1").unwrap(), 7.)
            .unwrap();
        let recovered = overlay.into_workbook();
        assert_eq!(recovered.sheets().as_ptr(), sheets);
        assert_eq!(
            recovered
                .stored_cell("Data", "A1".parse().unwrap())
                .unwrap()
                .value,
            CellValue::Number(2.)
        );
    }
}
