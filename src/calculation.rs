//! Direct and incremental calculation, semantics, contexts, and resource limits.
//!
//! ```
//! use poirot::{Workbook, CellValue, calculation::EvaluationContext};
//! let mut book = Workbook::new();
//! let sheet = book.add_sheet("Data")?;
//! sheet.set_value("A1", 2)?;
//! sheet.set_formula("B1", "A1*3")?;
//! let mut scenario = book.into_overlay().build()?;
//! let output = scenario.target("Data!B1")?;
//! let context = EvaluationContext::default();
//! assert_eq!(scenario.evaluation(&context)?.cell(output)?, CellValue::Number(6.));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
/// Application functions, registration contracts, and their budgeted runtime host.
///
/// Register an [`ExtensionFunction`] with [`ExtensionRegistry::register`], declaring
/// its argument, capability, and cacheability contract. Names cannot override builtins.
/// Attach the registry through [`EvaluationBuilder::extensions`] or
/// [`OverlayBuilder::extensions`]; use their `shared_extensions` methods to share
/// ownership. For individual runs, pass an [`ExtensionExecution`] to
/// [`run_cell_with_extensions`] or [`run_formula_with_extensions`].
///
/// See the [provider registration recipe](crate::eval::functions::extensions)
/// for a complete implementation and host contracts. Its engine builders use
/// `with_extensions`; the application builders above use `extensions`.
///
/// Native snapshots store provider identity, not executable providers. Reattach
/// providers with [`crate::snapshots::RestoreBuilder::shared_extensions`] before
/// restoring custom-function preparation. Reusing prepared state requires the
/// same [`ExtensionIdentity`], including registration revisions and external-name
/// identity; update the registration revision when its implementation changes.
///
/// [`ExtensionFunction`]: extensions::ExtensionFunction
/// [`ExtensionRegistry::register`]: extensions::ExtensionRegistry::register
/// [`EvaluationBuilder::extensions`]: EvaluationBuilder::extensions
/// [`OverlayBuilder::extensions`]: OverlayBuilder::extensions
/// [`ExtensionExecution`]: extensions::ExtensionExecution
/// [`run_cell_with_extensions`]: extensions::run_cell_with_extensions
/// [`run_formula_with_extensions`]: extensions::run_formula_with_extensions
/// [`ExtensionIdentity`]: extensions::ExtensionIdentity
pub mod extensions {
    pub use crate::eval::functions::extensions::{
        ExtensionBinding, ExtensionCacheability, ExtensionCapabilities, ExtensionContract,
        ExtensionError, ExtensionEvaluationStrategy, ExtensionFunction, ExtensionHost, ExtensionId,
        ExtensionIdentity, ExtensionInvocation, ExtensionNamespace, ExtensionOperandDescription,
        ExtensionOperandKind, ExtensionOutput, ExtensionRegistrationError,
        ExtensionRegistrationIdentity, ExtensionRegistry, ExtensionResult, ExtensionScalar,
        ExtensionTrace, REGISTERABLE_ATP_PLACEHOLDERS,
    };
    pub use crate::eval::functions::external_namespace::{
        ExternalFunctionNamespace, ExternalFunctionNamespaceIdentity, ExternalNameLookup,
        ExternalNamespaceError,
    };
    pub use crate::eval::functions::registry::Arity;
    pub use crate::eval::{
        ExtensionEvaluation, ExtensionExecution, run_cell_with_extensions,
        run_formula_with_extensions,
    };
}

use crate::eval;
use crate::files::XlsxSaveBuilder;
use crate::model::{self, CellValue, ModelError, Workbook, WorkbookChange};
use std::borrow::{Borrow, BorrowMut};

pub use crate::calculation_error::CalculationError;
pub use crate::eval::overlay::{
    CompilationUsage, EvaluationCycle, EvaluationReport, OverlayEdits, OverlayError,
};
pub use crate::eval::{
    ArrayResultPolicy, BudgetError, BudgetKind, CapabilityId, CurrencyContext, DateOrder,
    EvaluationLimits, EvaluationUsage, FormattingConfiguration, Localization, MemoRetentionLimits,
    MemoRetentionUsage, ProducerPolicy, RandomAccess, RegexSettings, ResourceFailureKind,
};
pub use crate::eval::{
    CellEvaluation, EvaluationContext, EvaluationError, EvaluationPolicy, EvaluationTarget,
    NumberLocale, PreparationPolicy,
};
pub use crate::eval::{
    EvaluationBudgets, EvaluationInputs, EvaluationOutcome, RunConfiguration, RunUsage, run_cell,
    run_cell_with_configuration, run_formula, run_formula_with_configuration, run_workbook,
    run_workbook_with_configuration,
};
pub use poirot_parsing::formula::FormulaParsePolicy;

/// Configure a direct evaluation over a borrowed or owned source without copying it.
///
/// [`Self::build`] captures one context and starts a cycle without evaluating cells.
/// Complete context replacement precedes any subsequent individual limits override.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1", "A1*3")?;
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
    inner: eval::overlay::EvaluationBuilder<'a, W>,
    context: Option<EvaluationContext>,
    profiling: bool,
    limits: Option<eval::EvaluationLimits>,
}
impl<'a, W: Borrow<model::Workbook>> EvaluationBuilder<'a, W> {
    /// Configure an ordinary calculation over its original source handle.
    pub(crate) fn new(workbook: W) -> Self {
        Self {
            inner: eval::overlay::Evaluation::with_workbook(workbook),
            context: None,
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
        self.inner = self.inner.with_policy(policy);
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
    /// Borrow the runtime providers used to bind and execute custom functions.
    pub fn extensions(
        mut self,
        registry: &'a eval::functions::extensions::ExtensionRegistry,
    ) -> Self {
        self.inner = self.inner.with_extensions(registry);
        self
    }
    /// Share runtime providers with an owned or worker-transferable scenario.
    pub fn shared_extensions(
        mut self,
        registry: std::sync::Arc<eval::functions::extensions::ExtensionRegistry>,
    ) -> Self {
        self.inner = self.inner.with_shared_extensions(registry);
        self
    }
    /// Capture context once and create a direct calculation without evaluating any cells.
    pub fn build(self) -> Result<Evaluation<'a, W>, OverlayError> {
        let mut context = self.context.unwrap_or_default();
        if let Some(limits) = self.limits {
            context = context.limits(limits);
        }
        Ok(Evaluation {
            inner: self
                .inner
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
/// it without copying. Use [`Self::with_workbook`] for a shared source and registry
/// configuration. Targets remain bound to that source's sheet topology and may be
/// invalidated by structural edits.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1", "A1*3")?;
/// let mut run = book.into_evaluation().build()?;
/// let output = run.target("Data!B1")?;
/// run.set_value(run.target("Data!A1")?, 5.)?;
/// assert_eq!(run.cell(output)?, CellValue::Number(15.));
/// let captured = run.save_xlsx().to_buffer()?;
/// let original = run.into_workbook();
/// assert!(original.stored_cell("Data", "A1")?.is_none());
/// let exported = Workbook::from_bytes(&captured)?;
/// assert_eq!(exported.stored_cell("Data", "B1")?.ok_or("missing result")?.value,
///     CellValue::Number(15.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Evaluation<'a, W = &'a model::Workbook> {
    inner: eval::overlay::Calculation<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> Evaluation<'a, W> {
    /// Configure a scenario over an owned, borrowed, or shared baseline without copying it.
    pub fn with_workbook(workbook: W) -> EvaluationBuilder<'a, W> {
        EvaluationBuilder::new(workbook)
    }

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
    pub fn target(
        &self,
        reference: &str,
    ) -> Result<EvaluationTarget, crate::eval::overlay::OverlayTargetError> {
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
    /// Replace authored style and clear results while preserving edits, work, and failure.
    pub fn set_style(
        &mut self,
        target: EvaluationTarget,
        style: model::CellStyle,
    ) -> Result<(), ModelError> {
        self.inner.set_style(target, style)
    }
    /// Calculate one cell and its dependencies in this cycle.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationError> {
        self.inner.cell(target)
    }
    /// Calculate selected outputs in caller order within this cycle.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<CellEvaluation>, EvaluationError> {
        self.inner.outputs(targets)
    }
    /// Calculate all effective stored cells and produced spills.
    pub fn all(&mut self) -> Result<Vec<CellEvaluation>, EvaluationError> {
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
    pub fn retained_usage(&self) -> eval::MemoRetentionUsage {
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
        eval::artifact::SnapshotSaveBuilder::checkpoint(self.inner.snapshot())
    }

    /// Add file export to an existing borrowed, owned, or shared calculation without copying it.
    pub fn from_calculation(inner: eval::overlay::Calculation<'a, W>) -> Self {
        Self { inner }
    }
    /// Capture effective authored edits and valid results now, without calculating or changing state.
    /// Later edits do not change the returned builder's captured workbook.
    /// Untouched saved formula results are preserved and may be stale.
    pub fn save_xlsx(&self) -> XlsxSaveBuilder<'static> {
        XlsxSaveBuilder::capture(self.inner.workbook(), self.inner.changes())
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
/// book.add_sheet("Data")?.set_formula("B1", "A1*3")?;
/// use poirot::calculation::EvaluationPolicy;
/// let policy = EvaluationPolicy::default();
/// let mut scenario = book.into_overlay().policy(policy).build()?;
/// let input = scenario.target("Data!A1")?;
/// scenario.set_value(input, 4.)?;
/// assert_eq!(scenario.apply_edits_to_workbook()?, 1);
/// let book = scenario.into_workbook();
/// assert_eq!(book.stored_cell("Data", "A1")?.ok_or("missing input")?.value,
///     CellValue::Number(4.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct OverlayBuilder<'a, W = &'a model::Workbook> {
    inner: eval::overlay::OverlayBuilder<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> OverlayBuilder<'a, W> {
    /// Configure an incremental workspace over its original source handle.
    pub(crate) fn new(workbook: W) -> Self {
        Self {
            inner: eval::overlay::Overlay::with_workbook(workbook),
        }
    }
    /// Replace spreadsheet semantics shared with direct evaluation.
    pub fn policy(mut self, policy: EvaluationPolicy) -> Self {
        self.inner = self.inner.with_policy(policy);
        self
    }
    /// Borrow the runtime providers used to bind and execute custom functions.
    pub fn extensions(
        mut self,
        registry: &'a eval::functions::extensions::ExtensionRegistry,
    ) -> Self {
        self.inner = self.inner.with_extensions(registry);
        self
    }
    /// Share runtime providers with an owned or worker-transferable scenario.
    pub fn shared_extensions(
        mut self,
        registry: std::sync::Arc<eval::functions::extensions::ExtensionRegistry>,
    ) -> Self {
        self.inner = self.inner.with_shared_extensions(registry);
        self
    }
    /// Create incremental state; select context and profiling explicitly for each cycle.
    pub fn build(self) -> Result<Overlay<'a, W>, OverlayError> {
        Ok(Overlay {
            inner: self.inner.build()?,
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
/// without copying. Use [`Self::with_workbook`] for a shared source and registry
/// configuration. Targets remain tied to the source sheet topology.
///
/// ```
/// use poirot::Workbook;
/// use poirot::model::CellValue;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_formula("B1", "A1*3")?;
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
/// assert_eq!(book.stored_cell("Data", "B1")?.ok_or("missing result")?.value,
///     CellValue::Number(15.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Overlay<'a, W = &'a model::Workbook> {
    inner: eval::overlay::Overlay<'a, W>,
}
impl<'a, W: Borrow<model::Workbook>> Overlay<'a, W> {
    /// Configure a scenario over an owned, borrowed, or shared baseline without copying it.
    pub fn with_workbook(workbook: W) -> OverlayBuilder<'a, W> {
        OverlayBuilder::new(workbook)
    }

    /// Resolve a reusable sheet-qualified address, such as `Data!A1` or `'Annual summary'!B12`.
    ///
    /// Invalid references and missing sheets return an error. Unstored coordinates
    /// on an existing sheet are valid targets; they calculate as blank when no
    /// source, scenario edit or spill supplies a value. Resolution does not
    /// calculate; targets remain tied to the source sheet topology.
    pub fn target(
        &self,
        reference: &str,
    ) -> Result<EvaluationTarget, crate::eval::overlay::OverlayTargetError> {
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
    pub fn compiled_usage(&mut self) -> CompilationUsage {
        self.inner.compiled_usage()
    }
    /// Prepare all effective formulas without executing them.
    pub fn compile_all(&mut self) -> CompilationUsage {
        self.inner.compile_all()
    }
    /// Inspect retained memo storage independently of profiling.
    pub fn retained_usage(&mut self) -> eval::MemoRetentionUsage {
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
        eval::artifact::SnapshotSaveBuilder::checkpoint(self.inner.snapshot())
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
    pub fn save_xlsx(&mut self) -> XlsxSaveBuilder<'static> {
        let changes = self.inner.changes();
        XlsxSaveBuilder::capture(self.inner.workbook(), changes)
    }
    /// Take the existing overlay without copying source or clearing warm state.
    pub fn into_overlay(self) -> eval::overlay::Overlay<'a, W> {
        self.inner
    }
}

impl Evaluation<'_, model::Workbook> {
    /// Recover the baseline, discarding unapplied edits and calculated results.
    pub fn into_workbook(self) -> Workbook {
        self.inner.into_workbook()
    }
}

impl Overlay<'_, model::Workbook> {
    /// Recover the baseline; explicitly apply authored edits first to preserve them.
    pub fn into_workbook(self) -> Workbook {
        self.inner.into_workbook()
    }
}

impl<W: Borrow<model::Workbook> + BorrowMut<model::Workbook>> Overlay<'_, W> {
    /// Apply authored edits atomically to an exclusively owned baseline, retaining valid results.
    pub fn apply_edits_to_workbook(&mut self) -> Result<usize, ModelError> {
        self.inner.apply_edits_to_workbook()
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    /// Build a source whose arithmetic results follow directly from its two inputs.
    fn workbook() -> Workbook {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("Data").unwrap();
        sheet.set_value("A1", 2.).unwrap();
        sheet.set_formula("B1", "A1*3").unwrap();
        workbook
    }

    #[test]
    fn borrowed_builders_keep_the_original_model() {
        let workbook = workbook();
        let source = &workbook as *const model::Workbook;
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
            recovered.stored_cell("Data", "A1").unwrap().unwrap().value,
            CellValue::Number(2.)
        );
        assert_eq!(
            recovered.stored_cell("Data", "B1").unwrap().unwrap().value,
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
                .get("B1")
                .unwrap()
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
            recovered.stored_cell("Data", "A1").unwrap().unwrap().value,
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
            recovered.stored_cell("Data", "A1").unwrap().unwrap().value,
            CellValue::Number(2.)
        );
        assert_eq!(
            recovered.stored_cell("Data", "B1").unwrap().unwrap().value,
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
            recovered.stored_cell("Data", "A1").unwrap().unwrap().value,
            CellValue::Number(2.)
        );
    }
}
