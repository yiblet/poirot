use super::compilation_usage::CompilationUsage;
use super::evaluation_cycle::EvaluationCycle;
use super::overlay_builder::OverlayBuilder;
use super::overlay_edits::OverlayEdits;
use super::overlay_error::OverlayError;
use super::overlay_evaluation::OverlayEvaluation;
use super::source::{Source, source_methods};
use super::workspace_state::WorkspaceState;
use super::{overlay_evaluation, snapshot, source};
use crate::eval::compile::compiler::FormulaCompiler;
use crate::eval::config::{EvaluationContext, EvaluationPolicy};
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::reuse::Cache;
use crate::model::{Cell, CellStyle, CellValue, ModelError, Workbook, WorkbookChange};
use std::{borrow::Borrow, cell::RefCell, sync::Arc};

/// Editable workbook scenarios that reuse calculations across input changes.
///
/// Input edits invalidate affected results while retaining unrelated results.
/// Edits and intermediate results remain available between evaluations.
/// Input edits leave the baseline unchanged until explicitly applied or flushed.
/// Select outputs through [`Overlay::evaluation`].
pub struct Overlay<'a, W = &'a Workbook> {
    pub(super) source: Source<'a, W>,
    pub(super) cache: RefCell<Cache>,
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

    /// Start an ordinary cycle without detailed profiling. Valid cached results survive.
    /// The context is borrowed for the cycle; starting another cycle explicitly resets work.
    pub fn calculation<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
    ) -> Result<EvaluationCycle<'e>, OverlayError> {
        self.calculation_with_profiling(context, false)
    }

    /// Start a cycle with independently selected detailed profiling and configured quotas.
    pub fn calculation_with_profiling<'e>(
        &'e mut self,
        context: &'e EvaluationContext,
        profiling: bool,
    ) -> Result<EvaluationCycle<'e>, OverlayError> {
        Ok(EvaluationCycle {
            inner: OverlayEvaluation::configured(self, context, profiling)?,
            profiling,
        })
    }

    /// Recover the original source handle, discarding unflushed edits and calculated results.
    pub fn into_workbook(self) -> W {
        self.source.workbook
    }

    source_methods!();

    /// Apply source edits and invalidate their combined dependency closure once.
    ///
    /// The closure receives only write operations; evaluation and cache reads are
    /// unavailable until it returns. Successful edits remain applied if the closure
    /// returns an error or unwinds. Their invalidation still completes before the
    /// overlay can be used again. There is no rollback or separate commit step.
    pub fn edit<R>(&mut self, apply: impl FnOnce(&mut OverlayEdits<'_, 'a, W>) -> R) -> R {
        let mut edits = OverlayEdits::new(&mut self.source, self.cache.get_mut());
        apply(&mut edits)
    }

    /// Discard pending authored edits and calculated results, keeping the baseline unchanged.
    pub fn discard_edits(&mut self) {
        self.reset();
    }

    /// Clear calculated results while retaining authored edits and compiled preparation.
    pub fn clear_calculated_cache(&mut self) {
        self.clear_cache();
    }

    pub(super) fn policy_changed(&mut self, previous: &EvaluationPolicy) {
        let policy = &self.source.policy;
        if previous.parse != policy.parse
            || previous.arrays != policy.arrays
            || previous.producers != policy.producers
        {
            self.clear_cache();
        } else if previous.byte_text_language != policy.byte_text_language {
            self.cache.get_mut().invalidate_byte_text_language();
        }
    }

    pub(super) fn source_changed(
        &mut self,
        target: EvaluationTarget,
        change: crate::eval::overlay::source_change::SourceChange,
    ) {
        let cache = self.cache.get_mut();
        match change {
            crate::eval::overlay::source_change::SourceChange::Value => {
                cache.invalidate_values([(target.sheet, target.address)])
            }
            crate::eval::overlay::source_change::SourceChange::Metadata => {
                cache.invalidate_geometry([(target.sheet, target.address)])
            }
        }
    }

    /// Discard edits and evaluated values while retaining compiled programs.
    pub fn reset(&mut self) {
        self.source.discard_inputs();
        self.clear_cache();
    }

    /// Clear calculated results and their dependencies, keeping edits and compiled formulas.
    pub fn clear_cache(&mut self) {
        self.cache = RefCell::new(Cache::default());
    }

    /// Inspect persistent storage, including dirty evidence awaiting revalidation.
    pub fn retained_usage(&mut self) -> crate::eval::limits::MemoRetentionUsage {
        let cache = self.cache.get_mut();
        crate::eval::limits::MemoRetentionUsage {
            cells: cache.len(),
            bytes: cache.retained_bytes,
        }
    }

    /// Count current formula results and spill children; dirty retained evidence is excluded.
    pub fn cached_cells(&mut self) -> usize {
        self.cache.get_mut().valid_len()
    }

    /// Inspect a current cached result without executing; dirty evidence remains hidden.
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
        let mut entries = cache
            .entries
            .iter()
            .filter(|(_, entry)| entry.validity.is_clean())
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|(key, _)| **key);
        for (&key, entry) in entries {
            if let Some(change) = overlay_evaluation::evaluated_change(
                self.source.effective(),
                key,
                &entry.value,
                entry.dependencies.spill,
            ) {
                changes.push(change);
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
    ) -> Result<(Workbook, WorkspaceState), OverlayError> {
        snapshot.restore()
    }
}

impl<W: Borrow<Workbook> + std::borrow::BorrowMut<Workbook>> Overlay<'_, W> {
    /// Atomically apply pending authored edits, preserving calculated results in the overlay.
    /// This does not write calculated results to the baseline or save a file.
    pub fn apply_edits_to_workbook(&mut self) -> Result<usize, ModelError> {
        self.flush()
    }

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
        let mut cells = Vec::new();
        let mut styles = Vec::new();
        for (sheet, address, edit) in self.source.inputs.edits() {
            match edit {
                crate::eval::inputs::CellEdit::Delete => cells.push((sheet, address, None)),
                crate::eval::inputs::CellEdit::StyleOnly(cell) => {
                    address.validate()?;
                    if self.source.workbook.borrow().sheet(sheet).is_none() {
                        return Err(ModelError::InvalidWorksheetIndex { index: sheet });
                    }
                    styles.push((sheet, address, cell.style.clone()));
                }
                crate::eval::inputs::CellEdit::Replace(cell) => {
                    cells.push((sheet, address, Some(cell.as_ref().clone())))
                }
            }
        }
        self.source.workbook.borrow_mut().replace_cells(cells)?;
        // Every style target was validated above. Cell replacement cannot change
        // worksheet membership, and set_style performs no other fallible work.
        for (sheet, address, style) in styles {
            self.source
                .workbook
                .borrow_mut()
                .sheet_mut(sheet)
                .ok_or(ModelError::InvalidWorksheetIndex { index: sheet })?
                .set_style(address, style)?;
        }
        // The baseline now contains the effective formulas, so dropping the diff
        // preserves source classification for these committed edits.
        self.source.discard_inputs();
        Ok(count)
    }
}
