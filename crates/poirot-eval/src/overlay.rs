//! Temporary cell edits and reusable results over an unchanged workbook.
//!
//! `Source` combines the baseline with edits and compiled formulas. `Cache` stores
//! results and the reads used to invalidate them when an input changes.
use crate::{EvaluationFailure, EvaluationTarget, FormulaCompiler};
use poirot_model::{
    Cell, CellAddress, CellStore, CellStyle, CellValue, ModelError, Workbook, WorkbookChange,
};
use std::{borrow::Borrow, cell::RefCell, collections::BTreeMap, sync::Arc};
mod context;
mod target;
use context::ContextKey;
// Evaluation inputs and overlay computation policies.
pub use context::{EvaluationContext, OverlayPolicy};
/// Consumed-read tracking and persistent result invalidation.
pub(crate) mod dependencies;
use dependencies::{Cache, Cached, Dependencies, Key, Reads};
mod evaluation;
/// Bounded execution borrowing an overlay's retained state.
pub use evaluation::OverlayEvaluation;
mod continuation;
mod direct;
mod output;
mod source;
// Editable direct workspaces without retained dependency tracking.
pub use direct::{Evaluation, EvaluationBuilder};
use source::{Registry, Source, source_methods};
/// Serializable views of workbook, edits, compiled formulas, and cached results.
pub mod snapshot;

/// Cumulative compilation work, independent of formula execution counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompilationUsage {
    /// Number of formula parsing attempts.
    pub parse_attempts: u64,
    /// Number of syntax-to-handler binding attempts.
    pub bind_attempts: u64,
    /// Number of exact compiled formula cache hits.
    pub reused_formulas: u64,
    /// Number of bytecode template reuse hits.
    pub template_hits: u64,
    /// Number of distinct retained bytecode programs.
    pub unique_programs: usize,
}
/// Failure to construct, restore, or operate on an overlay.
#[derive(Debug)]
pub enum OverlayError {
    /// Invalid workbook target or authored cell content.
    Model(ModelError),
    /// Operational evaluation or policy failure.
    Evaluation(EvaluationFailure),
    /// Invalid or incompatible serialized state.
    Snapshot(String),
    /// The supplied extension registry does not match the snapshot's identity.
    RegistryMismatch,
}
impl std::fmt::Display for OverlayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Model(e) => e.fmt(f),
            Self::Evaluation(e) => e.fmt(f),
            Self::Snapshot(e) => write!(f, "overlay snapshot: {e}"),
            Self::RegistryMismatch => f.write_str("overlay requires its exact extension registry"),
        }
    }
}
impl std::error::Error for OverlayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Model(error) => Some(error),
            Self::Evaluation(error) => Some(error),
            _ => None,
        }
    }
}
impl From<ModelError> for OverlayError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
impl From<EvaluationFailure> for OverlayError {
    fn from(e: EvaluationFailure) -> Self {
        Self::Evaluation(e)
    }
}

/// Temporary workbook inputs with dependency tracking for repeated calculations.
///
/// Input edits invalidate affected results while retaining unrelated results.
/// Input edits leave the baseline unchanged until explicitly applied or flushed.
/// Select outputs through [`Overlay::evaluation`].
pub struct Overlay<'a, W = &'a Workbook> {
    source: Source<'a, W>,
    cache: RefCell<Cache>,
}
/// Configures an overlay's workbook, formula policies, and custom functions.
pub struct OverlayBuilder<'a, W> {
    workbook: W,
    policy: OverlayPolicy,
    extensions: Option<Registry<'a>>,
}
impl<'a, W: Borrow<Workbook>> Overlay<'a, W> {
    /// Begin an overlay over a borrowed, shared, or owned workbook.
    pub fn with_workbook(workbook: W) -> OverlayBuilder<'a, W> {
        OverlayBuilder {
            workbook,
            policy: Default::default(),
            extensions: None,
        }
    }
    source_methods!();
    fn policy_changed(&mut self, previous: &OverlayPolicy) {
        let policy = &self.source.policy;
        if previous.parse != policy.parse
            || previous.builtin != policy.builtin
            || previous.arrays != policy.arrays
        {
            self.clear_cache();
        } else if previous.text != policy.text {
            self.cache.get_mut().invalidate_text_policy();
        }
    }
    fn source_changed(&mut self, target: EvaluationTarget) {
        self.cache
            .get_mut()
            .invalidate([(target.sheet, target.address)]);
    }
    /// Discard edits and evaluated values while retaining compiled programs.
    pub fn reset(&mut self) {
        self.source.inputs = crate::run_overlay::RunInputs::empty();
        self.clear_cache();
    }
    /// Clear calculated results and their dependencies, keeping edits and compiled formulas.
    pub fn clear_cache(&mut self) {
        self.cache = RefCell::new(Cache::default());
    }
    /// Count retained formula results and spill children.
    pub fn cached_cells(&mut self) -> usize {
        self.cache.get_mut().len()
    }
    /// Inspect a retained result without triggering execution.
    pub fn cached_value(&self, target: EvaluationTarget) -> Option<Arc<CellValue>> {
        self.cache
            .try_borrow()
            .ok()?
            .get((target.sheet, target.address))
            .map(|entry| Arc::clone(&entry.value))
    }
    /// Start a calculation with the supplied context.
    ///
    /// Results affected by changed context settings or volatile functions are
    /// invalidated before calculation begins.
    pub fn evaluation<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
    ) -> Result<OverlayEvaluation<'e>, OverlayError> {
        OverlayEvaluation::new(self, context)
    }
    /// Configure native persistence of authored edits and calculation preparation.
    pub fn save_snapshot(&self) -> crate::artifact::SnapshotSaveBuilder<'_> {
        crate::artifact::SnapshotSaveBuilder::checkpoint(self.snapshot())
    }
    /// Borrow the workbook, edits, compiled formulas, and cached results for serialization.
    pub fn snapshot(&self) -> snapshot::Snapshot<'_> {
        snapshot::Snapshot::borrowed(&self.source, Some(&self.cache))
    }
    /// Collect input edits and calculated results as workbook changes.
    pub fn changes(&mut self) -> Vec<WorkbookChange> {
        let cache = self.cache.get_mut();
        let mut changes = cache
            .spill_resets
            .iter()
            .map(|&(sheet, anchor)| WorkbookChange::ClearSpillCache { sheet, anchor })
            .collect::<Vec<_>>();
        changes.extend(self.source.source_changes());
        for (&sheet, cells) in &cache.sheets {
            for (&address, entry) in cells.cells() {
                if let Some(change) = evaluation::evaluated_change(
                    self.source.effective(),
                    (sheet, address),
                    &entry.value,
                    entry.dependencies.spill,
                    self.source.policy.materialization,
                ) {
                    changes.push(change);
                }
            }
        }
        changes
    }
    /// Consume the workspace and export all authored edits and retained result changes.
    pub fn into_changes(mut self) -> Vec<WorkbookChange> {
        self.changes()
    }
}
impl Overlay<'static, &'static Workbook> {
    /// Restore an owned baseline and detached overlay state; choose its handle on attachment.
    pub fn from_snapshot(
        snapshot: snapshot::Snapshot<'_>,
    ) -> Result<(Workbook, snapshot::WorkspaceState), OverlayError> {
        snapshot.restore()
    }
}
impl<W: Borrow<Workbook> + std::borrow::BorrowMut<Workbook>> Overlay<'_, W> {
    /// Apply input edits to the baseline, keeping compiled formulas and cached results.
    ///
    /// If validation fails, the baseline and edits are unchanged. On success, the
    /// applied edits are removed from the overlay. Affected results were already
    /// invalidated when the edits were made, so flushing does not invalidate them again.
    ///
    /// Only edited cells are staged; calculated results stay in the overlay.
    /// Before exporting a file, calculate the required outputs and apply
    /// [`Self::changes`] to a separate workbook to include those results.
    /// [`Self::snapshot`] includes them without changing the baseline.
    /// Edited spill anchors lose their saved children; edits to saved children
    /// replace that child's derived ownership with authored content. Other saved
    /// caches are left alone and never authorize evaluator reuse.
    ///
    /// Requires an owned or exclusively borrowed baseline (`BorrowMut<Workbook>`).
    /// Shared immutable handles continue to support edits and change export.
    pub fn flush(&mut self) -> Result<usize, ModelError> {
        let count = self.edited_cells();
        if count == 0 {
            return Ok(0);
        }
        let mut cells = self
            .source
            .inputs
            .deleted
            .iter()
            .map(|&(sheet, address)| (sheet, address, None))
            .collect::<Vec<_>>();
        for (&sheet, overrides) in &self.source.inputs.sheets {
            cells.extend(
                overrides
                    .cells()
                    .iter()
                    .map(|(&address, cell)| (sheet, address, Some(cell.clone()))),
            );
        }
        self.source.workbook.borrow_mut().replace_cells(cells)?;
        self.source.inputs = crate::run_overlay::RunInputs::empty();
        Ok(count)
    }
}
impl<'a, W: Borrow<Workbook>> OverlayBuilder<'a, W> {
    /// Select formula preparation, computation, and materialization policies.
    pub fn with_policy(mut self, policy: OverlayPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Borrow the exact registry used to compile and execute extension calls.
    pub fn with_extensions(mut self, registry: &'a crate::extensions::ExtensionRegistry) -> Self {
        self.extensions = Some(Registry::Borrowed(registry));
        self
    }
    /// Share providers so an already-used workspace can move to an ordinary worker thread.
    pub fn with_shared_extensions(
        mut self,
        registry: Arc<crate::extensions::ExtensionRegistry>,
    ) -> Self {
        self.extensions = Some(Registry::Shared(registry));
        self
    }
    /// Validate policies and create empty authored and result diffs.
    pub fn build(self) -> Result<Overlay<'a, W>, OverlayError> {
        Ok(Overlay {
            source: Source::new(self.workbook, self.policy, self.extensions)?,
            cache: RefCell::new(Cache::default()),
        })
    }
}
fn same_stored_value(a: &CellValue, b: &CellValue) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b) && crate::same_result_identity(a, b)
}
fn same_cell(a: &Cell, b: &Cell) -> bool {
    a.formula == b.formula && a.style == b.style && same_stored_value(&a.value, &b.value)
}
fn append_cell_changes(
    changes: &mut Vec<WorkbookChange>,
    (sheet, address): Key,
    original: Option<&Cell>,
    cell: &Cell,
) {
    let formula_changed = original.and_then(|c| c.formula.as_deref()) != cell.formula.as_deref();
    if formula_changed && let Some(formula) = &cell.formula {
        changes.push(WorkbookChange::SetFormula {
            sheet,
            address,
            formula: formula.clone(),
        });
    }
    if cell.formula.is_none() {
        if original.is_none_or(|c| c.formula.is_some() || !same_stored_value(&c.value, &cell.value))
        {
            changes.push(WorkbookChange::SetValue {
                sheet,
                address,
                value: cell.value.clone(),
            });
        }
    } else if (!formula_changed
        && original.is_none_or(|c| !same_stored_value(&c.value, &cell.value)))
        || (formula_changed && !matches!(cell.value, CellValue::Blank))
    {
        changes.push(WorkbookChange::SetCachedValue {
            sheet,
            address,
            // SAFETY: The preceding if handles formula.is_none(); this else branch therefore has Some, and formula is not mutated here.
            expected_formula: cell.formula.clone().unwrap(),
            value: cell.value.clone(),
        });
    }
    if original.map(|c| &c.style).unwrap_or(&CellStyle::default()) != &cell.style {
        changes.push(WorkbookChange::SetStyle {
            sheet,
            address,
            style: cell.style.clone(),
        });
    }
}
fn result_change(
    source: Option<&Cell>,
    (sheet, address): Key,
    value: &CellValue,
    materialization: crate::FormulaCacheMaterializationPolicy,
) -> Option<WorkbookChange> {
    // An implicit blank is an evaluation value, not authored source. Reads may
    // memoize it, but only source edits or spill publication can create a cell.
    // Authored blank edits are emitted separately by source_changes; formula
    // cells remain present here and publish their completed worksheet result.
    if source.is_none() && matches!(value, CellValue::Blank) {
        return None;
    }
    // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
    let stored = materialization.coerce_stored_value(value.clone());
    if source.is_some_and(|c| same_stored_value(&c.value, &stored)) {
        return None;
    }
    Some(match source.and_then(|c| c.formula.as_ref()) {
        Some(formula) => WorkbookChange::SetCachedValue {
            sheet,
            address,
            expected_formula: formula.clone(),
            value: stored,
        },
        None => WorkbookChange::SetValue {
            sheet,
            address,
            value: stored,
        },
    })
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_flush;
