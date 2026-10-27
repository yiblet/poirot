//! Admission of finished cell results and their effects on the active spill attempt.
use super::admission;
use super::computed_cell::ComputedCell;
use super::self_read::SelfRead;
use super::settlement::unsupported;
use crate::eval::functions::extensions;
use crate::eval::limits::{EvalResult, EvaluationFailure, WorkKind};
use crate::eval::ports::{CellCompletion, DependencyRead};
use crate::eval::session::ReadRecorder;
use crate::eval::session::Session;
use crate::eval::values::{ArrayView, VALUE_ERROR};
use crate::model::CellValue::{Blank, Error};
use crate::model::{CellAddress, CellError, CellValue};

/// Publishes one cell's computed result into run storage for spill ownership.
///
/// It stages array footprints against blockers, admits results through the
/// storage-only admission port, delivers completions, and records which reads an
/// active settlement attempt must be able to replay or roll back.
pub(in crate::eval) struct SpillPublisher<'s, 'a> {
    session: &'s mut Session<'a>,
}

impl<'s, 'a> SpillPublisher<'s, 'a> {
    /// Borrow the session for one publication step.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Stage a producer's spill after checking blockers and ownership.
    pub(in crate::eval) fn stage(
        &mut self,
        sheet: usize,
        at: CellAddress,
        rows: ArrayView,
    ) -> EvalResult<ComputedCell> {
        if !self
            .session
            .state
            .runtime
            .stack
            .visiting
            .contains(&(sheet, at))
        {
            return Err(unsupported(
                sheet,
                at,
                "dynamic array publication requires a source formula cell; ad hoc probes are unsupported",
            ));
        }
        let width = rows.first().map_or(0, Vec::len);
        if width == 0 || rows.iter().any(|row| row.len() != width) {
            return Ok(ComputedCell::scalar(Error(VALUE_ERROR)));
        }
        let height = rows.len();
        let last = at
            .row
            .checked_add(u32::try_from(height - 1).unwrap_or(u32::MAX))
            .zip(
                at.col
                    .checked_add(u32::try_from(width - 1).unwrap_or(u32::MAX)),
            )
            .map(|(row, col)| CellAddress { row, col });
        if last.is_none_or(|last| last.validate().is_err()) {
            return Ok(ComputedCell::scalar(Error(CellError::Spill)));
        }
        let count = height
            .checked_mul(width)
            .ok_or_else(|| unsupported(sheet, at, "array shape overflow"))?;
        self.session.state.runtime.budget.check_allocation(count)?;
        // SAFETY: The preceding last.is_none_or guard returns on None before this unwrap; last is not reassigned.
        let last = last.unwrap();
        ReadRecorder::new(self.session).read_spill_membership(
            sheet,
            CellAddress { row: 0, col: 0 },
            last,
        );
        let effective = crate::eval::inputs::EffectiveWorkbook::new(
            self.session.sources.workbook,
            self.session.sources.inputs,
        );
        for r in 0..height {
            for c in 0..width {
                self.session
                    .state
                    .runtime
                    .budget
                    .charge(WorkKind::ReferencePair, 1)?;
                // SAFETY: The validated last coordinate proves every r < height
                // and c < width fits the grid and both coordinate additions.
                let child = CellAddress {
                    row: at.row + r as u32,
                    col: at.col + c as u32,
                };
                let competing_owner = self
                    .session
                    .state
                    .runtime
                    .preparation
                    .roles
                    .get(&(sheet, child))
                    .is_some_and(|role| role.owner(child) != Some(at));
                if !competing_owner
                    && let Some(conflict) = self
                        .session
                        .state
                        .runtime
                        .preparation
                        .producer_reads
                        .self_read((sheet, at), (sheet, child))
                {
                    return Err(unsupported(
                        sheet,
                        at,
                        if conflict == SelfRead::Sparse {
                            "array result reads its own unpublished sparse footprint"
                        } else {
                            "array result reads its own unpublished footprint"
                        },
                    ));
                }
                if child != at
                    && (effective
                        .cell(sheet, child)
                        .is_some_and(|cell| cell.formula.is_some() || cell.value != Blank))
                {
                    return Ok(ComputedCell::scalar(Error(CellError::Spill)));
                }
            }
        }
        let first = rows
            .first()
            .and_then(|row| row.first())
            .ok_or(EvaluationFailure::StalePlan)?
            .clone();
        Ok(ComputedCell {
            value: first,
            rows: Some(rows),
        })
    }

    /// Admit a complete result through a port with no workbook or execution access.
    pub(in crate::eval) fn admit(
        &mut self,
        sheet: usize,
        at: CellAddress,
        result: ComputedCell,
    ) -> EvalResult<CellValue> {
        let runtime = &mut *self.session.state.runtime;
        admission::Admission {
            budget: &mut runtime.budget,
            cache: &mut runtime.memo.cache,
            preparation: &mut runtime.preparation,
            admitted: &mut runtime.memo.admitted,
            memo_usage: &mut runtime.memo.usage,
            consumer: runtime.stack.consumer,
            limits: self.session.state.memo_limits,
            journal: self.session.reuse.overlay_reads.is_some(),
        }
        .admit(sheet, at, result)
    }

    /// Deliver completion through the preparation state without exposing other session services.
    pub(in crate::eval) fn complete(&mut self, completion: CellCompletion<'_>) -> EvalResult<()> {
        let runtime = &mut *self.session.state.runtime;
        runtime.preparation.complete_cell(
            &mut runtime.budget,
            runtime.stack.consumer,
            &mut self.session.ports.cell_observer,
            completion,
        )
    }

    /// Actual external contracts, rather than conservative static effects, govern safe retries.
    pub(in crate::eval) fn record_effect(&mut self, read: &DependencyRead) {
        let runtime = &mut *self.session.state.runtime;
        let non_repeatable = match read {
            DependencyRead::OpaqueProvider(_) => true,
            DependencyRead::Extension(invocation) => matches!(
                invocation.binding.registration.contract.cacheability,
                extensions::ExtensionCacheability::Volatile
                    | extensions::ExtensionCacheability::Opaque
            ),
            _ => false,
        };
        if let Some(active) = runtime.preparation.settlement.active_mut() {
            if non_repeatable {
                active.non_repeatable = true;
            }
            if let Some(consumer) = runtime.stack.consumer {
                active.consumers.insert((consumer.sheet, consumer.address));
            }
            if let DependencyRead::Random(random) = read
                && let Ok(sheet) = usize::try_from(random.sheet)
            {
                active.random_touched.insert((
                    sheet,
                    CellAddress {
                        row: random.row,
                        col: random.column,
                    },
                ));
            }
        }
    }
}
