//! Recursive cell reads: source resolution, memo and cache reuse, execution and completion.

use super::bookkeeping::{
    copy_cached_value, record_external_hit, record_memo_hit, record_overlay_completion,
};
use super::cell_attempt::CellAttempt;
use super::completion_context::CompletionContext;
use super::resolved_source::ResolvedSource;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvalResult, EvaluationError, WorkKind};
use crate::eval::ports::CellProvenance;
use crate::eval::session::composition::Session;
use crate::eval::session::reads::ReadRecorder;
use crate::eval::spill::SpillQuery;
use crate::eval::values::signed_reference::GridRead;
use crate::eval::values::{CellEvaluation, numeric_outcome};
use crate::model::CellValue::{Blank, Number};
use crate::model::{CellAddress, CellValue};

/// Reads or evaluates cells, recording dependencies, spill effects and completion provenance.
pub(in crate::eval) struct CellEvaluator<'s, 'a> {
    /// The run whose state this executor reads and updates.
    session: &'s mut Session<'a>,
}

impl<'s, 'a> CellEvaluator<'s, 'a> {
    /// Borrow the session for this executor's calls.
    pub(in crate::eval) fn new(session: &'s mut Session<'a>) -> Self {
        Self { session }
    }

    /// Read an encountered range member without memoizing authored nonblank literals.
    /// Observation ports and spill-owned content retain the ordinary cell protocol.
    pub(in crate::eval) fn range_cell(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> EvalResult<CellValue> {
        let source = ResolvedSource::resolve(self.session, sheet, address);
        self.range_cell_from_source(sheet, address, source)
    }

    /// Apply the ordinary range read protocol to a source retained by sparse traversal.
    pub(in crate::eval) fn range_cell_from_source(
        &mut self,
        sheet: usize,
        address: CellAddress,
        source: Option<ResolvedSource<'a>>,
    ) -> EvalResult<CellValue> {
        if self.session.sources.extensions.is_some()
            || self.session.ports.dependency_sink.is_some()
            || self.session.ports.cell_reader.is_some()
            || self.session.ports.cell_observer.is_some()
            || self.session.ports.builtin_invocation_observer.is_some()
            || self.session.ports.execution_observer.is_some()
            || self.session.state.runtime.preparation.failed()
            || self
                .session
                .state
                .runtime
                .preparation
                .has_admitted_role((sheet, address))
        {
            return self.cell_from_source(sheet, address, source);
        }
        let Some(cell) = source.and_then(|source| {
            source.cell.filter(|cell| {
                cell.formula.is_none()
                    && !matches!(cell.value, Blank)
                    && !source.accepts_spill
                    && (self.session.policies.array_result_policy
                        == ArrayResultPolicy::DynamicSpill
                        || self
                            .session
                            .sources
                            .workbook
                            .sheet(sheet)
                            .is_some_and(|worksheet| worksheet.spill_owner(address).is_none()))
            })
        }) else {
            return self.cell_from_source(sheet, address, source);
        };
        let target = EvaluationTarget { sheet, address };
        self.session
            .state
            .runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(target);
                failure
            })?;
        ReadRecorder::new(self.session).read_cell(target);
        self.session
            .state
            .runtime
            .budget
            .enter_depth()
            .map_err(|mut failure| {
                failure.set_target(target);
                failure
            })?;
        let budget = &mut self.session.state.runtime.budget;
        let value = if let Some(text) = cell.value.text_ref() {
            // Only a text copy can incur a charge attributed to this literal.
            budget.push_cell(target);
            let value = budget
                .text(text.storage_bytes())
                .map(|()| cell.value.clone());
            budget.pop_cell();
            value
        } else {
            Ok(match &cell.value {
                Number(value) => numeric_outcome::convert_numeric_outcome(*value),
                value => value.clone(),
            })
        };
        budget.leave_depth();
        value
    }

    /// Read or evaluate a cell, recording dependencies, spill effects, and completion provenance.
    pub(in crate::eval) fn cell(
        &mut self,
        sheet: usize,
        address: CellAddress,
    ) -> EvalResult<CellValue> {
        let source = ResolvedSource::resolve(self.session, sheet, address);
        self.cell_from_source(sheet, address, source)
    }

    /// Calculate a worksheet cell and reuse its resolved source for output formatting.
    pub(in crate::eval) fn output(
        &mut self,
        target: EvaluationTarget,
    ) -> EvalResult<CellEvaluation> {
        let source = ResolvedSource::resolve(self.session, target.sheet, target.address);
        let value = self.cell_from_source(target.sheet, target.address, source)?;
        Ok(CellEvaluation::from_source(
            target,
            value,
            source.and_then(|source| source.cell),
        ))
    }

    /// Whether a caller-ordered batch can try its authored roots against the local memo.
    pub(in crate::eval) fn has_local_output_cache(&self) -> bool {
        let session = &*self.session;
        let runtime = &*session.state.runtime;
        !runtime.memo.cache.is_empty()
            && runtime.stack.consumer.is_none()
            && !runtime.preparation.failed()
            && !SpillQuery::new(session).settling()
            && session.ports.dependency_sink.is_none()
            && session.ports.cell_observer.is_none()
            && session.reuse.overlay_reads.is_none()
    }

    /// Project a memoized authored root with the same read charges as ordinary execution.
    pub(in crate::eval) fn local_output(
        &mut self,
        target: EvaluationTarget,
    ) -> Option<EvalResult<CellEvaluation>> {
        let source = ResolvedSource::resolve(self.session, target.sheet, target.address)?;
        if source.accepts_spill {
            return None;
        }
        let runtime = &mut *self.session.state.runtime;
        let value = runtime.memo.cache.get(&(target.sheet, target.address))?;
        Some((|| {
            runtime
                .budget
                .charge(WorkKind::CellRead, 1)
                .map_err(|mut failure| {
                    failure.set_target(target);
                    failure
                })?;
            record_memo_hit(&mut runtime.budget, &mut runtime.run_usage);
            let value = copy_cached_value(&mut runtime.budget, target, value)?;
            Ok(CellEvaluation::from_source(target, value, source.cell))
        })())
    }

    /// Borrow clean overlay results before the batch begins any local computation.
    pub(in crate::eval) fn retained_output_cache(
        &self,
    ) -> Option<std::cell::Ref<'a, crate::eval::reuse::Cache>> {
        let session = &*self.session;
        let cache = session.reuse.overlay_cache?;
        let runtime = &*session.state.runtime;
        if session.policies.producer_policy != crate::eval::config::ProducerPolicy::Compute
            || runtime.stack.consumer.is_some()
            || !runtime.memo.cache.is_empty()
            || runtime.preparation.failed()
            || SpillQuery::new(session).settling()
            || session.ports.cell_reader.is_some()
            || session.ports.dependency_sink.is_some()
            || session.ports.cell_observer.is_some()
            || session.ports.execution_observer.is_some()
            || session.ports.builtin_invocation_observer.is_some()
            || session.reuse.overlay_reads.is_none()
        {
            return None;
        }
        let cache = cache.try_borrow().ok()?;
        (cache.len() != 0).then_some(cache)
    }

    /// Project one clean scalar retained result, keeping admission and writeback accounting.
    /// The caller stops using this path as soon as ordinary evaluation starts.
    pub(in crate::eval) fn retained_output(
        &mut self,
        cache: &crate::eval::reuse::Cache,
        target: EvaluationTarget,
    ) -> Option<EvalResult<CellEvaluation>> {
        let entry = cache.get((target.sheet, target.address))?;
        if entry.dependencies.spill != crate::eval::reuse::SpillRole::None
            || entry.dependencies.spill_sensitive
        {
            return None;
        }
        let source = ResolvedSource::resolve(self.session, target.sheet, target.address)?;
        let reads = self.session.reuse.overlay_reads.as_mut()?;
        let runtime = &mut *self.session.state.runtime;
        Some((|| {
            runtime
                .budget
                .charge(WorkKind::CellRead, 1)
                .map_err(|mut failure| {
                    failure.set_target(target);
                    failure
                })?;
            record_external_hit(runtime);
            let value = copy_cached_value(&mut runtime.budget, target, &entry.value)?;
            record_overlay_completion(reads, target, CellProvenance::ExternalCache)?;
            Ok(CellEvaluation::from_source(target, value, source.cell))
        })())
    }

    /// Preserve the ordinary read lifecycle around an already resolved immutable source.
    fn cell_from_source(
        &mut self,
        sheet: usize,
        address: CellAddress,
        source: Option<ResolvedSource<'a>>,
    ) -> EvalResult<CellValue> {
        let attempt = CellAttempt {
            target: EvaluationTarget { sheet, address },
            consumer: self.session.state.runtime.stack.consumer,
            provenance: CellProvenance::Evaluated,
            completion_candidate: false,
        };
        // A dependency formula has its own worksheet demand. Restore its caller's
        // demand before completion callbacks, on both success and operational failure.
        let demand = self.session.policies.root_demand();
        let previous_demand = std::mem::replace(&mut self.session.state.expression_demand, demand);
        let read = attempt.read(self.session, source);
        self.session.state.expression_demand = previous_demand;
        let settling_spills = SpillQuery::new(self.session).settling();
        if !settling_spills
            && self.session.reuse.overlay_reads.is_none()
            && self.session.ports.cell_observer.is_none()
        {
            return read.result;
        }
        let runtime = &mut *self.session.state.runtime;
        read.complete(CompletionContext {
            array_result_policy: self.session.policies.array_result_policy,
            settling_spills,
            preparation: &mut runtime.preparation,
            budget: &mut runtime.budget,
            visiting: &runtime.stack.visiting,
            reads: &mut self.session.reuse.overlay_reads,
            consumer: runtime.stack.consumer,
            observer: &mut self.session.ports.cell_observer,
        })
    }

    /// Settle only dirty immediate causes, keeping successful validation local until publication.
    pub(in crate::eval) fn revalidate_cached(
        &mut self,
        key: (usize, CellAddress),
    ) -> EvalResult<Option<CellValue>> {
        let Some(cache) = self.session.reuse.overlay_cache else {
            return Ok(None);
        };
        let mut evidence = {
            let cache = cache.try_borrow().map_err(|_| EvaluationError::StalePlan)?;
            let Some(entry) = cache.retained(key) else {
                return Ok(None);
            };
            let Some(causes) = entry
                .dependencies
                .ordered_causes(&entry.validity, &mut self.session.state.runtime.budget)?
            else {
                return Ok(None);
            };
            (std::sync::Arc::clone(&entry.value), causes)
        };
        // Validating evidence must not replace the original consumed dependency trace
        // with the narrower set of dirty causes, or leak these reads into a caller.
        let previous = self.session.state.runtime.stack.consumer.take();
        let unchanged: Result<bool, EvaluationError> = (|| {
            loop {
                let prior = {
                    let cache = cache.try_borrow().map_err(|_| EvaluationError::StalePlan)?;
                    let entry = cache.retained(key).ok_or(EvaluationError::StalePlan)?;
                    let Some((cause, expected_version)) = evidence
                        .1
                        .next(&entry.validity, &mut self.session.state.runtime.budget)?
                    else {
                        return Ok(true);
                    };
                    cache.retained(cause).map(|entry| {
                        (
                            cause,
                            expected_version,
                            entry.version,
                            std::sync::Arc::clone(&entry.value),
                            entry.validity.is_clean(),
                        )
                    })
                };
                let Some((cause, expected_version, version, prior, clean)) = prior else {
                    return Ok(false);
                };
                if version != expected_version {
                    return Ok(false);
                }
                self.session
                    .state
                    .runtime
                    .budget
                    .charge(WorkKind::Comparison, 1)?;
                if clean {
                    continue;
                }
                // Stop at the first changed value: normal execution decides which
                // old lazy dependencies are still relevant, and in what order.
                let unchanged = match self.session.state.runtime.memo.cache.get(&cause) {
                    Some(value) => crate::eval::ports::same_result_identity(&prior, value),
                    None => crate::eval::ports::same_result_identity(
                        &prior,
                        &self.cell(cause.0, cause.1)?,
                    ),
                };
                if !unchanged {
                    return Ok(false);
                }
            }
        })();
        self.session.state.runtime.stack.consumer = previous;
        if !unchanged? {
            return Ok(None);
        }
        let value = copy_cached_value(
            &mut self.session.state.runtime.budget,
            EvaluationTarget {
                sheet: key.0,
                address: key.1,
            },
            &evidence.0,
        )?;
        if let Some(reads) = &mut self.session.reuse.overlay_reads {
            reads.validated.insert(key);
        }
        Ok(Some(value))
    }

    /// Read an in-grid cell or produce a logical off-grid blank.
    pub(in crate::eval) fn signed_read(&mut self, plan: GridRead) -> EvalResult<CellValue> {
        match plan {
            GridRead::Blank => Ok(Blank),
            GridRead::Cell { sheet, address } => self.cell(sheet, address),
        }
    }

    /// Read a signed range member, borrowing eligible authored literals directly.
    pub(in crate::eval) fn signed_range_read(&mut self, plan: GridRead) -> EvalResult<CellValue> {
        match plan {
            GridRead::Blank => Ok(Blank),
            GridRead::Cell { sheet, address } => self.range_cell(sheet, address),
        }
    }
}
