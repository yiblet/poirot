//! One bounded execution lifetime borrowing an overlay's source, compiler and cache.
use super::*;

/// A fresh evaluation shares memoized dependencies and budgets across its output calls.
/// Each successful call publishes its complete batch. A failed call publishes no new
/// results and poisons this instance; create a new evaluation to retry.
pub struct OverlayEvaluation<'a> {
    session: crate::Session<'a>,
    cache: &'a RefCell<Cache>,
    completed: crate::coordinate_hash::CellSet,
    spill_resets: std::collections::BTreeSet<Key>,
    failure: Option<EvaluationFailure>,
    retention: crate::run::MemoRetentionLimits,
    materialization: crate::FormulaCacheMaterializationPolicy,
}
impl<'a> OverlayEvaluation<'a> {
    /// Start a bounded execution and invalidate results stale for its context.
    pub(super) fn new<W: Borrow<Workbook>>(
        overlay: &'a mut Overlay<'_, W>,
        context: &'a EvaluationContext,
    ) -> Result<Self, OverlayError> {
        overlay
            .source
            .policy
            .materialization
            .validate(overlay.source.policy.numeric)?;
        let meter = crate::budget::BudgetMeter::new(context.budgets)?;
        overlay.cache.get_mut().begin(context.key());
        let mut session = overlay.source.session(context, meter);
        session.overlay_reads = Some(Reads::default());
        session.overlay_cache = Some(&overlay.cache);
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
        self.perform(|session| session.cell(target.sheet, target.address))
    }
    /// Return requested outputs in caller order, including duplicates. Dependencies
    /// share this evaluation's memo; writeback is atomic across this entire list.
    pub fn outputs(
        &mut self,
        targets: &[EvaluationTarget],
    ) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.perform(|session| output::outputs(session, targets))
    }

    /// Evaluate all effective stored cells, plus any materialized spill children.
    /// Implicit blanks outside the sparse workbook are never expanded.
    pub fn all(&mut self) -> Result<Vec<crate::CellEvaluation>, EvaluationFailure> {
        self.perform(output::all)
    }

    /// Values reached in this instance, including dependency and spill cells.
    pub fn values(&self) -> impl Iterator<Item = (EvaluationTarget, &CellValue)> {
        self.session
            .cache
            .iter()
            .map(|(&(sheet, address), value)| (EvaluationTarget { sheet, address }, value))
    }
    /// Return execution counters accumulated across this instance's output calls.
    pub fn usage(&self) -> &crate::RunUsage {
        &self.session.run_usage
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
                    crate::run_overlay::EffectiveWorkbook::new(
                        self.session.workbook,
                        self.session.inputs,
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
    pub fn finish(self) -> crate::RunReport<()> {
        crate::RunReport {
            execution: self.session.run_usage,
            outcome: self
                .session
                .budget
                .outcome(self.failure.map_or(Ok(()), Err)),
        }
    }
    fn perform<T>(
        &mut self,
        evaluate: impl FnOnce(&mut crate::Session<'a>) -> Result<T, EvaluationFailure>,
    ) -> Result<T, EvaluationFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        // Move rather than clone the retained index. Only a successfully published
        // dependency batch may return it to the overlay; aborted work is discarded.
        self.session.lookup_prefixes = std::mem::take(
            &mut self
                .cache
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .lookup_prefixes,
        );
        self.session
            .lookup_prefixes
            .admit(self.session.budget.limits.materialized_values);
        let result = evaluate(&mut self.session).and_then(|value| {
            self.publish()?;
            self.session.lookup_prefixes.publish();
            self.cache
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .lookup_prefixes = std::mem::take(&mut self.session.lookup_prefixes);
            Ok(value)
        });
        if let Err(failure) = &result {
            self.failure = Some(failure.clone());
        }
        result
    }
    fn publish(&mut self) -> Result<(), EvaluationFailure> {
        let reads = self
            .session
            .overlay_reads
            .as_mut()
            .ok_or(EvaluationFailure::StalePlan)?;
        reads.flush();
        let mut evaluated = std::mem::take(&mut reads.evaluated);
        let mut completed = std::mem::take(&mut reads.completed);
        if self.session.array_result_policy == crate::ArrayResultPolicy::DynamicSpill {
            // Hydrated footprints retain their original dependency trace. Only
            // children of producers actually executed in this batch get new traces.
            let children = self
                .session
                .spill_owners
                .iter()
                .filter(|((sheet, _), owner)| evaluated.contains(&(*sheet, **owner)))
                .map(|(key, _)| *key)
                .collect::<Vec<_>>();
            evaluated.extend(children);
            completed.extend(self.session.spill_owners.keys().copied());
        }
        let effective =
            crate::run_overlay::EffectiveWorkbook::new(self.session.workbook, self.session.inputs);
        let mut staged = Vec::new();
        for key in evaluated {
            let Some(value) = self.session.cache.get(&key) else {
                continue;
            };
            let source = effective.cell(key.0, key.1);
            // Literal/absent reads need graph edges, but no retained copy of their values.
            if !self.session.spill_owners.contains_key(&key)
                && source.is_none_or(|cell| cell.formula.is_none())
                && source.map_or(matches!(value, CellValue::Blank), |cell| {
                    crate::same_result_identity(&cell.value, value)
                })
            {
                continue;
            }
            let mut dependencies = Box::new(reads.traces.remove(&key).unwrap_or_default());
            dependencies.spill =
                crate::spill_publication::spill_role(&self.session.spill_owners, key);
            if let crate::spill_publication::SpillRole::Child { anchor } = dependencies.spill {
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
                self.session
                    .spill_owners
                    .iter()
                    .filter(|((_, at), owner)| at == *owner)
                    .map(|(&(sheet, at), _)| (sheet, at)),
            );
        }
        completed.extend(staged.iter().map(|(key, _)| *key));
        self.cache
            .try_borrow_mut()
            .map_err(|_| EvaluationFailure::StalePlan)?
            .commit(
                staged,
                self.retention,
                self.session.consumer,
                resets.iter().copied(),
            )?;
        self.completed.extend(completed);
        self.spill_resets.extend(resets);
        Ok(())
    }
}

/// Build a guarded cache write, preserving spill ownership when present.
pub(super) fn evaluated_change(
    effective: crate::run_overlay::EffectiveWorkbook<'_>,
    key: Key,
    value: &CellValue,
    spill: crate::spill_publication::SpillRole,
    policy: crate::FormulaCacheMaterializationPolicy,
) -> Option<WorkbookChange> {
    if let crate::spill_publication::SpillRole::Child { anchor } = spill {
        let formula = effective.cell(key.0, anchor)?.formula.as_ref()?.clone();
        return Some(WorkbookChange::SetSpillCachedValue {
            sheet: key.0,
            address: key.1,
            anchor,
            expected_formula: formula,
            // TODO(coercion-non-argument): [MEDIUM] Cache publication can replace a computed number with an error; storing the converted value as a live result would change subsequent evaluation.
            value: policy.coerce_stored_value(value.clone()),
        });
    }
    result_change(effective.cell(key.0, key.1), key, value, policy)
}

#[cfg(test)]
mod safety_tests {
    use super::*;

    #[test]
    fn conflicting_cache_borrow_returns_stale_plan() {
        let workbook = Workbook::new();
        let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
        let context = EvaluationContext::default();
        let evaluation = overlay.evaluation(&context).unwrap();
        let _held = evaluation.cache.borrow_mut();
        assert_eq!(
            evaluation.result_changes(),
            Err(EvaluationFailure::StalePlan)
        );
    }
}
