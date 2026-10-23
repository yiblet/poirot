//! Movable run-local state; baseline and service borrows are reattached for each call.
use super::*;

/// Owned continuation of direct execution, with no dependency graph or runtime borrows.
pub(super) struct Continuation {
    bytecode_stacks: Vec<Vec<crate::Value>>,
    lookup_prefixes: crate::adapters::search::execution::LookupPrefixes,
    random_streams: crate::coordinate_hash::CellMap<(crate::random::Stream, usize)>,
    extension_trace: crate::extensions::ExtensionTrace,
    spill_discovery: std::collections::BTreeMap<usize, crate::spill_publication::Discovery>,
    spill_owners: crate::coordinate_hash::CellMap<CellAddress>,
    pending_spills: crate::coordinate_hash::CellMap<crate::ArrayView>,
    spill_read_ranges: crate::coordinate_hash::CellMap<Vec<crate::Area>>,
    blank_reads: crate::coordinate_hash::CellMap<crate::coordinate_hash::CellSet>,
    cache: crate::run::CellMemo,
    admitted: Vec<Key>,
    memo_usage: crate::run::MemoRetentionUsage,
    visiting: crate::coordinate_hash::CellSet,
    names: std::collections::HashSet<(usize, usize)>,
    numeric_date_effects: crate::coordinate_hash::CellMap<poirot_coercionv2::date_time::Effects>,
    budget: crate::budget::BudgetMeter,
    observation_state: crate::execution_observation::ObservationState,
    consumer: Option<EvaluationTarget>,
    run_usage: crate::RunUsage,
}
impl Continuation {
    /// Move completed execution state out of a temporarily borrowed engine.
    pub fn capture(session: crate::Session<'_>) -> Self {
        let crate::Session {
            bytecode_stacks,
            lookup_prefixes,
            random_streams,
            extension_trace,
            spill_discovery,
            spill_owners,
            pending_spills,
            spill_read_ranges,
            blank_reads,
            cache,
            admitted,
            memo_usage,
            visiting,
            names,
            numeric_date_effects,
            budget,
            observation_state,
            consumer,
            run_usage,
            // List borrows explicitly so adding engine state cannot silently lose it.
            extensions: _,
            workbook: _,
            inputs: _,
            context: _,
            currency: _,
            number_locale: _,
            text_policy: _,

            builtin_policy: _,
            array_result_policy: _,
            expression_demand: _,
            memo_limits: _,
            plan: _,
            lazy_compiler: _,
            preparation_policy: _,
            overlay_cache: _,
            overlay_reads: _,
            dependency_sink: _,
            cell_reader: _,
            cell_observer: _,
            builtin_invocation_observer: _,
            execution_observer: _,
        } = session;
        Self {
            bytecode_stacks,
            lookup_prefixes,
            random_streams,
            extension_trace,
            spill_discovery,
            spill_owners,
            pending_spills,
            spill_read_ranges,
            blank_reads,
            cache,
            admitted: admitted.unwrap_or_default(),
            memo_usage,
            visiting,
            names,
            numeric_date_effects,
            budget,
            observation_state,
            consumer,
            run_usage,
        }
    }
    /// Reattach immutable source and context without self-referential storage.
    pub fn resume<'e, W: Borrow<Workbook>>(
        &mut self,
        source: &'e Source<'_, W>,
        context: &'e EvaluationContext,
    ) -> crate::Session<'e> {
        let bytecode_stacks = std::mem::take(&mut self.bytecode_stacks);
        let lookup_prefixes = std::mem::take(&mut self.lookup_prefixes);
        let random_streams = std::mem::take(&mut self.random_streams);
        let extension_trace = std::mem::take(&mut self.extension_trace);
        let spill_discovery = std::mem::take(&mut self.spill_discovery);
        let spill_owners = std::mem::take(&mut self.spill_owners);
        let pending_spills = std::mem::take(&mut self.pending_spills);
        let spill_read_ranges = std::mem::take(&mut self.spill_read_ranges);
        let blank_reads = std::mem::take(&mut self.blank_reads);
        let cache = std::mem::take(&mut self.cache);
        let admitted = std::mem::take(&mut self.admitted);
        let memo_usage = std::mem::take(&mut self.memo_usage);
        let visiting = std::mem::take(&mut self.visiting);
        let names = std::mem::take(&mut self.names);
        let numeric_date_effects = std::mem::take(&mut self.numeric_date_effects);
        let budget = self.budget.take();
        let observation_state = std::mem::take(&mut self.observation_state);
        let consumer = std::mem::take(&mut self.consumer);
        let run_usage = std::mem::take(&mut self.run_usage);
        let mut session = source.session(context, budget);
        session.bytecode_stacks = bytecode_stacks;
        session.lookup_prefixes = lookup_prefixes;
        session.random_streams = random_streams;
        session.extension_trace = extension_trace;
        session.spill_discovery = spill_discovery;
        session.spill_owners = spill_owners;
        session.pending_spills = pending_spills;
        session.spill_read_ranges = spill_read_ranges;
        session.blank_reads = blank_reads;
        session.cache = cache;
        session.admitted = Some(admitted);
        session.memo_usage = memo_usage;
        session.visiting = visiting;
        session.names = names;
        session.numeric_date_effects = numeric_date_effects;
        session.observation_state = observation_state;
        session.consumer = consumer;
        session.run_usage = run_usage;
        session
    }
    /// Forget derived values while retaining cumulative work budgets and counters.
    pub fn clear_results(&mut self) {
        self.cache.clear();
        self.admitted.clear();
        self.spill_discovery.clear();
        self.spill_owners.clear();
        self.pending_spills.clear();
        self.spill_read_ranges.clear();
        self.blank_reads.clear();
        self.visiting.clear();
        self.names.clear();
        self.random_streams.clear();
        self.numeric_date_effects.clear();
        self.extension_trace = Default::default();
        self.lookup_prefixes = Default::default();
        self.lookup_prefixes.enable();
        self.memo_usage = Default::default();
        self.consumer = None;
    }
    /// Successful or failed work accumulated in this evaluation identity.
    pub fn usage(&self) -> &crate::RunUsage {
        &self.run_usage
    }
    /// Read the local value memo; no incremental evidence is attached.
    pub fn values(&self) -> &crate::run::CellMemo {
        &self.cache
    }
    /// Export one result with guarded spill ownership.
    pub fn change(
        &self,
        source: crate::run_overlay::EffectiveWorkbook<'_>,
        key: Key,
        policy: crate::FormulaCacheMaterializationPolicy,
    ) -> Option<WorkbookChange> {
        let value = self.cache.get(&key)?;
        evaluation::evaluated_change(
            source,
            key,
            value,
            crate::spill_publication::spill_role(&self.spill_owners, key),
            policy,
        )
    }
    /// Finish this evaluation with actual-work budget information.
    pub fn finish(self, failure: Option<EvaluationFailure>) -> crate::RunReport<()> {
        crate::RunReport {
            execution: self.run_usage,
            outcome: self.budget.outcome(failure.map_or(Ok(()), Err)),
        }
    }
}
