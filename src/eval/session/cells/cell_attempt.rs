//! One cell read attempt: admission, reuse, spill resolution and execution.

use super::bookkeeping::{cell_allocation_failure, copy_cached_value};
use super::cell_evaluator::CellEvaluator;
use super::cell_read::CellRead;
use super::cell_reuse::CellReuse;
use super::formula_execution::FormulaExecution;
use super::resolved_source::ResolvedSource;
use crate::eval::config::ArrayResultPolicy;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::{EvalResult, WorkKind};
use crate::eval::ports::{CellProvenance, DependencyRead, ReadEvent};
use crate::eval::session::composition::Session;
use crate::eval::session::projection::ResultProjection;
use crate::eval::session::reads::ReadRecorder;
use crate::eval::spill;
use crate::eval::spill::{SpillDemand, SpillPublisher, SpillQuery};
use crate::eval::values::numeric_outcome;
use crate::model::CellError::Ref;
use crate::model::CellValue::{Blank, Error, Number};
use crate::model::{CellAddress, CellValue};

/// One cell read's attribution and publication eligibility, retained even on failure.
pub(super) struct CellAttempt {
    pub(super) target: EvaluationTarget,
    pub(super) consumer: Option<EvaluationTarget>,
    pub(super) provenance: CellProvenance,
    pub(super) completion_candidate: bool,
}

impl CellAttempt {
    /// Run admission, reuse and execution while preserving failure attribution.
    pub(super) fn read<'a>(
        mut self,
        session: &mut Session<'a>,
        source: Option<ResolvedSource<'a>>,
    ) -> CellRead {
        let result = self.resolve_and_read(session, source);
        CellRead {
            attempt: self,
            result,
        }
    }

    /// Admit an effective source read before consulting caches or executing dependencies.
    fn resolve_and_read<'a>(
        &mut self,
        session: &mut Session<'a>,
        source: Option<ResolvedSource<'a>>,
    ) -> EvalResult<CellValue> {
        let Some(source) = source else {
            return Ok(Error(Ref));
        };
        session
            .state
            .runtime
            .budget
            .charge(WorkKind::CellRead, 1)
            .map_err(|mut failure| {
                failure.set_target(self.target);
                failure
            })?;
        ReadRecorder::new(session).read_lazy(|| DependencyRead::Cell(self.target));
        match self.reuse(session, source)? {
            CellReuse::Value(value) => Ok(value),
            CellReuse::Execute { needs_validation } => {
                self.execute(session, source, needs_validation)
            }
        }
    }

    /// Preserve cache precedence and resolve incoming spill ownership before ordinary reuse.
    fn reuse<'a>(
        &mut self,
        session: &mut Session<'a>,
        source: ResolvedSource<'a>,
    ) -> EvalResult<CellReuse> {
        let EvaluationTarget { sheet, address } = self.target;
        let incoming = source.accepts_spill;
        // A clean unowned blank certificate already includes its empty-owner
        // query guard. Owned coordinates still restore a whole footprint for
        // coherent export and spill-reference readiness. Active arbitration and
        // locally admitted values retain their ordinary protocol.
        if incoming
            && !SpillQuery::new(session).settling()
            && !session
                .state
                .runtime
                .memo
                .cache
                .contains_key(&(sheet, address))
            && let Some(value) = session.reuse.overlay_cache.and_then(|cache| {
                let cache = cache.try_borrow().ok()?;
                let entry = cache.get((sheet, address))?;
                if entry.dependencies.spill.owner(address).is_some() {
                    return None;
                }
                Some(std::sync::Arc::clone(&entry.value))
            })
        {
            self.completion_candidate = true;
            self.external_hit(session);
            return copy_cached_value(
                &mut session.state.runtime.budget,
                self.target,
                value.as_ref(),
            )
            .map(CellReuse::Value);
        }
        self.resolve_spills(session, source)?;
        self.completion_candidate = incoming;
        let runtime = &mut *session.state.runtime;
        if let Some(value) = runtime.memo.cache.get(&(sheet, address)) {
            self.provenance = CellProvenance::SessionMemo;
            if runtime.budget.profiling {
                runtime.run_usage.session_memo_hits =
                    runtime.run_usage.session_memo_hits.saturating_add(1);
                runtime.budget.usage.cache_hits = runtime.budget.usage.cache_hits.saturating_add(1);
            }
            return copy_cached_value(&mut runtime.budget, self.target, value)
                .map(CellReuse::Value);
        }
        if session.sources.inputs.is_none()
            && let Some(reader) = &mut session.ports.cell_reader
            && let Some(value) = reader
                .cached_cell(self.target)
                .inspect_err(|_| self.provenance = CellProvenance::ExternalCache)?
        {
            self.external_hit(session);
            return copy_cached_value(
                &mut session.state.runtime.budget,
                self.target,
                value.as_ref(),
            )
            .map(CellReuse::Value);
        }
        let (cached, needs_validation) = session
            .reuse
            .overlay_cache
            .and_then(|cache| {
                let cache = cache.try_borrow().ok()?;
                let entry = cache.retained((sheet, address))?;
                Some((
                    entry
                        .validity
                        .is_clean()
                        .then(|| std::sync::Arc::clone(&entry.value)),
                    matches!(
                        entry.validity,
                        crate::eval::reuse::Validity::Cause { .. }
                            | crate::eval::reuse::Validity::Causes(_)
                    ),
                ))
            })
            .unwrap_or_default();
        if let Some(value) = cached {
            self.external_hit(session);
            return copy_cached_value(
                &mut session.state.runtime.budget,
                self.target,
                value.as_ref(),
            )
            .map(CellReuse::Value);
        }
        Ok(CellReuse::Execute { needs_validation })
    }

    /// Retain cache provenance before copying a value, including a copy-budget failure.
    fn external_hit(&mut self, session: &mut Session<'_>) {
        self.provenance = CellProvenance::ExternalCache;
        if session.state.runtime.budget.profiling {
            session.state.runtime.run_usage.external_cache_hits = session
                .state
                .runtime
                .run_usage
                .external_cache_hits
                .saturating_add(1);
            session.state.runtime.budget.usage.cache_hits = session
                .state
                .runtime
                .budget
                .usage
                .cache_hits
                .saturating_add(1);
        }
    }

    /// Attribute ownership queries to the coordinate, then restore the consuming cell.
    fn resolve_spills<'a>(
        &self,
        session: &mut Session<'a>,
        source: ResolvedSource<'a>,
    ) -> EvalResult<()> {
        let EvaluationTarget { sheet, address } = self.target;
        let incoming = source.accepts_spill;
        // Empty coordinates own their spatial-query dependency, including empty
        // query results. Their callers depend on the coordinate's actual value.
        let previous = if incoming {
            session.state.runtime.stack.consumer.replace(self.target)
        } else {
            session.state.runtime.stack.consumer
        };
        let resolution = SpillDemand::new(session).resolve_cell(sheet, address, source);
        if incoming && resolution.is_ok() {
            ReadRecorder::new(session).read_spill_membership(
                sheet,
                CellAddress { row: 0, col: 0 },
                address,
            );
            if let Some(owner) = session
                .state
                .runtime
                .preparation
                .admitted_role((sheet, address))
                .owner(address)
                && let Some(reads) = &mut session.reuse.overlay_reads
            {
                reads.record(&ReadEvent {
                    consumer: session.state.runtime.stack.consumer,
                    read: DependencyRead::Cell(EvaluationTarget {
                        sheet,
                        address: owner,
                    }),
                });
            }
        }
        session.state.runtime.stack.consumer = previous;
        resolution?;
        // Preserve the public read observer's owner notification for its caller;
        // retained incremental edges are attached to the coordinate node above.
        if incoming
            && let Some(sink) = &mut session.ports.dependency_sink
            && let Some(owner) = session
                .state
                .runtime
                .preparation
                .admitted_role((sheet, address))
                .owner(address)
        {
            sink.read(ReadEvent {
                consumer: previous,
                read: DependencyRead::Cell(EvaluationTarget {
                    sheet,
                    address: owner,
                }),
            });
        }
        Ok(())
    }

    /// Enter the recursive cell frame and restore its caller before returning any outcome.
    fn execute<'a>(
        &mut self,
        session: &mut Session<'a>,
        source: ResolvedSource<'a>,
        needs_validation: bool,
    ) -> EvalResult<CellValue> {
        let EvaluationTarget { sheet, address } = self.target;
        // Source and inputs are immutable for this execution. Spill publication
        // updates runtime ownership and values, never the authored source cell.
        let cell = source.cell;
        self.completion_candidate |= cell.is_some_and(|cell| cell.formula.is_some());
        // Scalar literals and absent cells cannot recurse. Dynamic spill execution
        // also uses visiting for ownership and footprint tracking, so retain its path.
        let track_visiting = session.policies.array_result_policy
            == ArrayResultPolicy::DynamicSpill
            || cell.is_some_and(|cell| cell.formula.is_some());
        if track_visiting
            && session
                .state
                .runtime
                .stack
                .visiting
                .contains(&(sheet, address))
        {
            if session.policies.array_result_policy == ArrayResultPolicy::DynamicSpill
                && SpillQuery::new(session).candidate(sheet, address)
            {
                return Err(crate::eval::spill::unsupported(
                    sheet,
                    address,
                    "dynamic execution encountered a circular unpublished footprint read",
                ));
            }
            return Ok(Error(Ref));
        }
        if track_visiting {
            session
                .state
                .runtime
                .stack
                .visiting
                .try_reserve(1)
                .map_err(|_| cell_allocation_failure(self.target))?;
            // SAFETY: One new coordinate slot was reserved above; coordinate hashing cannot panic.
            session
                .state
                .runtime
                .stack
                .visiting
                .insert((sheet, address));
        }
        if let Err(mut failure) = session.state.runtime.budget.enter_depth() {
            if track_visiting {
                session
                    .state
                    .runtime
                    .stack
                    .visiting
                    .remove(&(sheet, address));
            }
            failure.set_target(self.target);
            return Err(failure);
        }
        if let Err(failure) = session.state.runtime.budget.try_push_cell(self.target) {
            // SAFETY: enter_depth succeeded above, and this path has not left that depth.
            session.state.runtime.budget.leave_depth();
            if track_visiting {
                session
                    .state
                    .runtime
                    .stack
                    .visiting
                    .remove(&(sheet, address));
            }
            return Err(failure);
        }
        let previous = session.state.runtime.stack.consumer.replace(self.target);
        // All fallible work happens inside this call. No `?` can bypass restoration.
        let result = self.execute_source(session, source.cell, needs_validation);
        session.state.runtime.budget.pop_cell();
        session.state.runtime.stack.consumer = previous;
        // SAFETY: enter_depth succeeded above; execute_source balances only its nested calls.
        session.state.runtime.budget.leave_depth();
        if track_visiting {
            session
                .state
                .runtime
                .stack
                .visiting
                .remove(&(sheet, address));
        }
        session
            .state
            .runtime
            .preparation
            .producer_reads
            .forget((sheet, address));
        result
    }

    /// Validate retained evidence or compute authored content, then admit the result locally.
    fn execute_source(
        &mut self,
        session: &mut Session<'_>,
        cell: Option<&crate::model::Cell>,
        needs_validation: bool,
    ) -> EvalResult<CellValue> {
        let EvaluationTarget { sheet, address } = self.target;
        if needs_validation
            && let Some(value) = CellEvaluator::new(session).revalidate_cached((sheet, address))?
        {
            let value = SpillPublisher::new(session).admit(
                sheet,
                address,
                spill::ComputedCell::scalar(value),
            )?;
            self.external_hit(session);
            if let Some(reads) = &mut session.reuse.overlay_reads {
                reads
                    .fresh_results
                    .try_reserve(1)
                    .map_err(|_| cell_allocation_failure(self.target))?;
                // SAFETY: One result slot was reserved above; numeric coordinate hashing cannot panic.
                reads.fresh_results.insert((sheet, address));
            }
            return Ok(value);
        }
        let value = match cell {
            None => Ok(spill::ComputedCell::scalar(Blank)),
            Some(cell) => match &cell.formula {
                None => {
                    if let Some(text) = cell.value.text_ref() {
                        session.state.runtime.budget.text(text.storage_bytes())?;
                    }
                    Ok(spill::ComputedCell::scalar(match &cell.value {
                        // TODO(coercion-non-argument): [MEDIUM] Numeric fidelity policy can replace subnormals with zero or nonfinite numbers with errors before a downstream consumer sees them.
                        Number(value) => numeric_outcome::convert_numeric_outcome(*value),
                        value => value.clone(),
                    }))
                }
                Some(formula) => {
                    session.state.runtime.budget.text(formula.len())?;
                    if session.state.runtime.budget.profiling {
                        session.state.runtime.run_usage.formula_executions = session
                            .state
                            .runtime
                            .run_usage
                            .formula_executions
                            .saturating_add(1);
                    }
                    let compiled;
                    let prepared = match session
                        .sources
                        .plan
                        .and_then(|plan| plan.formula(self.target))
                    {
                        Some(prepared) => prepared.as_ref(),
                        None => {
                            compiled = session.sources.prepare_source(formula);
                            compiled.as_ref()
                        }
                    };
                    let value = FormulaExecution::new(session)
                        .prepared_expression(sheet, address, prepared)?;
                    ResultProjection::new(session).publish_worksheet_result(sheet, address, value)
                }
            },
        }?;
        SpillPublisher::new(session).admit(sheet, address, value)
    }
}
