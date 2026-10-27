//! One bounded execution lifetime borrowing an overlay's source, compiler and cache.
use super::changes::result_change;
use super::incremental::Overlay;
use super::output;
use super::overlay_error::OverlayError;
use crate::eval::config::EvaluationContext;
use crate::eval::coordinates::EvaluationTarget;
use crate::eval::limits::EvaluationFailure;
use crate::eval::reuse::{Cache, Cached, Key, Reads};
use crate::eval::session::CellEvaluator;
use crate::model::{CellValue, Workbook, WorkbookChange};
use std::borrow::Borrow;
use std::cell::RefCell;
use std::sync::Arc;

/// A fresh evaluation shares memoized dependencies and budgets across its output calls.
/// Each successful call publishes its complete batch. A failed call publishes no new
/// results and poisons this instance; create a new evaluation to retry.
pub struct OverlayEvaluation<'a> {
    pub(super) session: crate::eval::session::Session<'a>,
    pub(super) cache: &'a RefCell<Cache>,
    pub(super) completed: crate::eval::coordinates::CellSet,
    spill_resets: std::collections::BTreeSet<Key>,
    failure: Option<EvaluationFailure>,
    pub(super) retention: crate::eval::limits::MemoRetentionLimits,
    materialization: crate::eval::materialization::FormulaCacheMaterializationPolicy,
}

impl<'a> OverlayEvaluation<'a> {
    /// Start a bounded execution and invalidate results stale for its context.
    pub(super) fn new<W: Borrow<Workbook>>(
        overlay: &'a mut Overlay<'_, W>,
        context: &'a EvaluationContext,
    ) -> Result<Self, OverlayError> {
        Self::configured(overlay, context, true)
    }

    /// Validate retained admission and start a cycle with independently selected profiling.
    pub(super) fn configured<W: Borrow<Workbook>>(
        overlay: &'a mut Overlay<'_, W>,
        context: &'a EvaluationContext,
        profiling: bool,
    ) -> Result<Self, OverlayError> {
        let meter = crate::eval::limits::BudgetMeter::configured(context.limits.into(), profiling)?
            .with_optional_limits(context.limits);
        let retained = overlay.cache.get_mut();
        if retained.len() > context.retention.cells
            || retained.retained_bytes > context.retention.bytes
        {
            return Err(EvaluationFailure::MemoRetentionLimit {
                limits: context.retention,
                consumed: crate::eval::limits::MemoRetentionUsage {
                    cells: retained.len(),
                    bytes: retained.retained_bytes,
                },
                requested: Default::default(),
                source: None,
            }
            .into());
        }
        retained.begin(context.key());
        let mut session = overlay.source.session(context, meter);
        session.reuse.overlay_reads = Some(Reads::default());
        session.reuse.overlay_cache = Some(&overlay.cache);
        Ok(Self {
            session,
            cache: &overlay.cache,
            completed: Default::default(),
            spill_resets: Default::default(),
            failure: None,
            retention: context.retention,
            materialization: overlay.source.policy.materialization,
        })
    }

    /// Evaluate one output and atomically retain its successful dependency batch.
    pub fn cell(&mut self, target: EvaluationTarget) -> Result<CellValue, EvaluationFailure> {
        self.perform(|session| CellEvaluator::new(session).cell(target.sheet, target.address))
    }

    /// Return requested outputs in caller order, including duplicates. Dependencies
    /// share this evaluation's memo; writeback is atomic across this entire list.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationFailure> {
        self.perform(|session| output::outputs(session, targets))
    }

    /// Evaluate all effective stored cells, plus any materialized spill children.
    /// Implicit blanks outside the sparse workbook are never expanded.
    pub fn all(&mut self) -> Result<Vec<crate::eval::values::CellEvaluation>, EvaluationFailure> {
        self.perform(output::all)
    }

    /// Memoized values in this instance, including dependency and spill results.
    /// Authored literals read directly by range consumers need no memo entry.
    pub fn values(&self) -> impl Iterator<Item = (EvaluationTarget, &CellValue)> {
        self.session
            .state
            .runtime
            .memo
            .cache
            .iter()
            .map(|(&(sheet, address), value)| (EvaluationTarget { sheet, address }, value))
    }

    /// Return execution counters accumulated across this instance's output calls.
    pub fn usage(&self) -> &crate::eval::limits::RunUsage {
        &self.session.state.runtime.run_usage
    }

    /// Export completed results and spill cleanup as owned workbook cache changes.
    pub fn result_changes(&self) -> Result<Vec<WorkbookChange>, EvaluationFailure> {
        let cache = self
            .cache
            .try_borrow()
            .map_err(|_| EvaluationFailure::StalePlan)?;
        let mut completed = self.completed.iter().copied().collect::<Vec<_>>();
        completed.sort_unstable();
        Ok(self
            .spill_resets
            .iter()
            .map(|&(sheet, anchor)| WorkbookChange::ClearSpillCache { sheet, anchor })
            .chain(completed.into_iter().filter_map(|key| {
                let entry = cache.get(key)?;
                evaluated_change(
                    crate::eval::inputs::EffectiveWorkbook::new(
                        self.session.sources.workbook,
                        self.session.sources.inputs,
                    ),
                    key,
                    &entry.value,
                    entry.dependencies.spill,
                    self.materialization,
                )
            }))
            .collect())
    }

    /// Consume this evaluation and export its completed cache changes.
    pub fn into_result_changes(self) -> Result<Vec<WorkbookChange>, EvaluationFailure> {
        self.result_changes()
    }

    /// Finish execution and report accumulated work and any operational failure.
    pub fn finish(mut self) -> crate::eval::run::RunReport<()> {
        crate::eval::run::RunReport {
            execution: std::mem::take(&mut self.session.state.runtime.run_usage),
            outcome: self
                .session
                .state
                .runtime
                .budget
                .take()
                .outcome(self.failure.map_or(Ok(()), Err)),
        }
    }

    fn perform<T>(
        &mut self,
        evaluate: impl FnOnce(&mut crate::eval::session::Session<'a>) -> Result<T, EvaluationFailure>,
    ) -> Result<T, EvaluationFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        // Move rather than clone the retained index. Only a successfully published
        // dependency batch may return it to the overlay; aborted work is discarded.
        self.session.state.runtime.lookup_prefixes = std::mem::take(
            &mut self
                .cache
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .lookup_prefixes,
        );
        let limit = self.session.state.runtime.budget.limits.materialized_values;
        self.session.state.runtime.lookup_prefixes.admit(limit);
        let result = evaluate(&mut self.session).and_then(|value| {
            self.publish()?;
            self.cache
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .lookup_prefixes = std::mem::take(&mut self.session.state.runtime.lookup_prefixes);
            Ok(value)
        });
        if let Err(failure) = &result {
            self.failure = Some(failure.clone());
        }
        result
    }

    fn publish(&mut self) -> Result<(), EvaluationFailure> {
        let publication = self.session.state.runtime.preparation.publication()?;
        let reads = self
            .session
            .reuse
            .overlay_reads
            .as_mut()
            .ok_or(EvaluationFailure::StalePlan)?;
        reads.flush();
        let mut fresh_results = std::mem::take(&mut reads.fresh_results);
        let mut completed = std::mem::take(&mut reads.completed);
        for &key in publication.pending_cells() {
            let owner = publication.role(key).owner(key.1);
            if owner.is_some_and(|owner| fresh_results.contains(&(key.0, owner))) {
                fresh_results.insert(key);
            }
            completed.insert(key);
        }
        let effective = crate::eval::inputs::EffectiveWorkbook::new(
            self.session.sources.workbook,
            self.session.sources.inputs,
        );
        let mut staged = Vec::new();
        for key in fresh_results.drain() {
            let Some(value) = self.session.state.runtime.memo.cache.get(&key) else {
                continue;
            };
            let source = effective.cell(key.0, key.1);
            // Ordinary literals need no retained copy. Empty coordinates with
            // ownership-query guards retain values for downstream change cutoffs.
            if !reads
                .traces
                .get(&key)
                .is_some_and(|trace| !trace.spill_ranges.is_empty())
                && !publication.contains(key)
                && source.is_none_or(|cell| cell.formula.is_none())
                && source.map_or(matches!(value, CellValue::Blank), |cell| {
                    crate::eval::ports::same_result_identity(&cell.value, value)
                })
            {
                continue;
            }
            let mut dependencies = if reads.validated.remove(&key) {
                self.cache
                    .try_borrow()
                    .map_err(|_| EvaluationFailure::StalePlan)?
                    .retained(key)
                    .ok_or(EvaluationFailure::StalePlan)?
                    .dependencies
                    .clone()
            } else {
                Box::new(reads.traces.remove(&key).unwrap_or_default())
            };
            dependencies.spill = publication.role(key);
            if let crate::eval::reuse::SpillRole::Child { anchor } = dependencies.spill {
                dependencies.cells.push((key.0, anchor));
            }
            dependencies.cells.sort_unstable();
            dependencies.cells.dedup();
            // A membership guard already invalidates every value in its rectangle.
            // Do not store millions of redundant point reverse edges for dense scans.
            dependencies
                .cells
                .retain(|&(sheet, at)| !dependencies.ranges.iter().any(|r| r.contains(sheet, at)));
            let entry = Cached {
                version: 0,
                validity: crate::eval::reuse::Validity::Clean,
                value: Arc::new(value.clone()),
                dependencies,
            };
            staged.push((key, entry));
        }
        let mut resets = std::collections::BTreeSet::new();
        {
            resets.extend(
                staged
                    .iter()
                    .filter(|(key, _)| {
                        effective
                            .cell(key.0, key.1)
                            .is_some_and(|c| c.formula.is_some())
                    })
                    .map(|(key, _)| *key),
            );
            resets.extend(
                publication
                    .pending_cells()
                    .iter()
                    .filter(|key| {
                        matches!(
                            publication.role(**key),
                            crate::eval::reuse::SpillRole::Anchor { .. }
                        )
                    })
                    .copied(),
            );
        }
        completed.extend(staged.iter().map(|(key, _)| *key));
        self.cache
            .try_borrow_mut()
            .map_err(|_| EvaluationFailure::StalePlan)?
            .commit(
                staged,
                self.retention,
                self.session.state.runtime.stack.consumer,
                resets.iter().copied(),
            )?;
        self.completed.extend(completed.drain());
        self.spill_resets.extend(resets);
        // Retain only empty publication buffers after a successful atomic commit.
        // An error still discards the local batch and poisons this evaluation.
        reads.fresh_results = fresh_results;
        reads.completed = completed;
        self.session
            .state
            .runtime
            .preparation
            .acknowledge_publication();

        Ok(())
    }
}

/// Build a guarded cache write, preserving spill ownership when present.
pub(super) fn evaluated_change(
    effective: crate::eval::inputs::EffectiveWorkbook<'_>,
    key: Key,
    value: &CellValue,
    spill: crate::eval::reuse::SpillRole,
    policy: crate::eval::materialization::FormulaCacheMaterializationPolicy,
) -> Option<WorkbookChange> {
    if let crate::eval::reuse::SpillRole::Child { anchor } = spill {
        let formula = effective.cell(key.0, anchor)?.formula.as_ref()?.clone();
        return Some(WorkbookChange::SetSpillCachedValue {
            sheet: key.0,
            address: key.1,
            anchor,
            expected_formula: formula,
            // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
            value: policy.convert_stored_value(value.clone()),
        });
    }
    if let crate::eval::reuse::SpillRole::Anchor { last } = spill {
        return Some(WorkbookChange::SetSpillAnchorCachedValue {
            sheet: key.0,
            anchor: key.1,
            last,
            expected_formula: effective.cell(key.0, key.1)?.formula.as_ref()?.clone(),
            value: policy.convert_stored_value(value.clone()),
        });
    }
    result_change(effective.cell(key.0, key.1), key, value, policy)
}
