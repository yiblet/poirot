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
        Self::configured(overlay, context, true)
    }
    /// Validate retained admission and start a cycle with independently selected profiling.
    pub(super) fn configured<W: Borrow<Workbook>>(
        overlay: &'a mut Overlay<'_, W>,
        context: &'a EvaluationContext,
        profiling: bool,
    ) -> Result<Self, OverlayError> {
        let meter = crate::budget::BudgetMeter::configured(context.budgets, profiling)?
            .with_optional_limits(context.evaluation_limits());
        let retained = overlay.cache.get_mut();
        if retained.len() > context.retention.cells
            || retained.retained_bytes > context.retention.bytes
        {
            return Err(EvaluationFailure::MemoRetentionLimit {
                limits: context.retention,
                consumed: crate::run::MemoRetentionUsage {
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
            .runtime
            .cache
            .iter()
            .map(|(&(sheet, address), value)| (EvaluationTarget { sheet, address }, value))
    }
    /// Return execution counters accumulated across this instance's output calls.
    pub fn usage(&self) -> &crate::RunUsage {
        &self.session.runtime.run_usage
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
    pub fn finish(mut self) -> crate::RunReport<()> {
        crate::RunReport {
            execution: std::mem::take(&mut self.session.runtime.run_usage),
            outcome: self
                .session
                .runtime
                .budget
                .take()
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
        self.session.runtime.lookup_prefixes = std::mem::take(
            &mut self
                .cache
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .lookup_prefixes,
        );
        let limit = self.session.runtime.budget.limits.materialized_values;
        self.session.runtime.lookup_prefixes.admit(limit);
        let result = evaluate(&mut self.session).and_then(|value| {
            self.publish()?;
            self.cache
                .try_borrow_mut()
                .map_err(|_| EvaluationFailure::StalePlan)?
                .lookup_prefixes = std::mem::take(&mut self.session.runtime.lookup_prefixes);
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
        let mut fresh_results = std::mem::take(&mut reads.fresh_results);
        let mut completed = std::mem::take(&mut reads.completed);
        let mut journal = std::mem::take(&mut self.session.runtime.preparation.journal);
        for &key in &journal {
            let owner = self
                .session
                .runtime
                .preparation
                .roles
                .get(&key)
                .and_then(|role| role.owner(key.1));
            if owner.is_some_and(|owner| fresh_results.contains(&(key.0, owner))) {
                fresh_results.insert(key);
            }
            completed.insert(key);
        }
        let effective =
            crate::run_overlay::EffectiveWorkbook::new(self.session.workbook, self.session.inputs);
        let mut staged = Vec::new();
        for key in fresh_results.drain() {
            let Some(value) = self.session.runtime.cache.get(&key) else {
                continue;
            };
            let source = effective.cell(key.0, key.1);
            // Literal/absent reads need graph edges, but no retained copy of their values.
            if !self.session.runtime.preparation.roles.contains_key(&key)
                && source.is_none_or(|cell| cell.formula.is_none())
                && source.map_or(matches!(value, CellValue::Blank), |cell| {
                    crate::same_result_identity(&cell.value, value)
                })
            {
                continue;
            }
            let mut dependencies = Box::new(reads.traces.remove(&key).unwrap_or_default());
            dependencies.spill = self
                .session
                .runtime
                .preparation
                .roles
                .get(&key)
                .copied()
                .unwrap_or_default();
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
                journal
                    .iter()
                    .filter(|key| {
                        matches!(
                            self.session.runtime.preparation.roles.get(key),
                            Some(crate::spill_publication::SpillRole::Anchor { .. })
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
                self.session.runtime.consumer,
                resets.iter().copied(),
            )?;
        self.completed.extend(completed.drain());
        self.spill_resets.extend(resets);
        // Retain only empty publication buffers after a successful atomic commit.
        // An error still discards the local batch and poisons this evaluation.
        reads.fresh_results = fresh_results;
        reads.completed = completed;
        journal.clear();
        self.session.runtime.preparation.journal = journal;

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
    if let crate::spill_publication::SpillRole::Anchor { last } = spill {
        return Some(WorkbookChange::SetSpillAnchorCachedValue {
            sheet: key.0,
            anchor: key.1,
            last,
            expected_formula: effective.cell(key.0, key.1)?.formula.as_ref()?.clone(),
            value: policy.coerce_stored_value(value.clone()),
        });
    }
    result_change(effective.cell(key.0, key.1), key, value, policy)
}

#[cfg(test)]
mod safety_tests {
    use super::*;

    #[test]
    fn separate_public_calls_preserve_complete_exports_while_reusing_empty_buffers() {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_sheet("S").unwrap();
        sheet
            .set_formula("A1".parse().unwrap(), "SEQUENCE(2,2)")
            .unwrap();
        sheet
            .set_formula("A4".parse().unwrap(), "SEQUENCE(2,2)")
            .unwrap();
        let mut overlay = Overlay::with_workbook(&workbook).build().unwrap();
        let context = EvaluationContext::default();
        let mut evaluation = overlay.evaluation(&context).unwrap();
        let target = |address: &str| EvaluationTarget {
            sheet: 0,
            address: address.parse().unwrap(),
        };
        assert_eq!(evaluation.cell(target("B2")).unwrap(), CellValue::from(4.));
        let first = evaluation.result_changes().unwrap();
        let reads = evaluation.session.overlay_reads.as_ref().unwrap();
        assert!(reads.fresh_results.is_empty());
        assert!(reads.completed.is_empty());
        assert!(reads.fresh_results.capacity() > 0);
        assert!(reads.completed.capacity() > 0);
        assert!(evaluation.session.runtime.preparation.journal.is_empty());
        assert!(evaluation.session.runtime.preparation.journal.capacity() > 0);
        assert_eq!(evaluation.cell(target("B5")).unwrap(), CellValue::from(4.));
        let changes = evaluation.result_changes().unwrap();
        assert!(first.iter().all(|change| changes.contains(change)));
        let mut published = workbook.clone();
        published.apply(changes).unwrap();
        let sheet = published.sheet(0).unwrap();
        for (anchor, first_row) in [("A1", 0), ("A4", 3)] {
            assert_eq!(
                sheet.spill_extent(anchor.parse().unwrap()),
                Some(CellAddress {
                    row: first_row + 1,
                    col: 1
                })
            );
            for row in 0..2 {
                for col in 0..2 {
                    assert_eq!(
                        sheet
                            .get(CellAddress {
                                row: first_row + row,
                                col
                            })
                            .unwrap()
                            .value,
                        CellValue::from(f64::from(row * 2 + col + 1))
                    );
                }
            }
        }
    }
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
