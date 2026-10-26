//! Persistent execution storage, independent of temporarily attached workbook ports.
use super::*;

/// Stacks, caches, budgets and effects retained across cell and output calls.
pub(in crate::eval) struct Runtime {
    pub(in crate::eval) vm: crate::eval::bytecode_vm::Machine,
    pub(in crate::eval) lookup_prefixes: adapters::search::execution::LookupPrefixes,
    pub(in crate::eval) random_streams: CellMap<(random::Stream, usize)>,
    pub(in crate::eval) extension_trace: extensions::ExtensionTrace,
    pub(in crate::eval) preparation: spill_publication::Preparation,
    pub(in crate::eval) cache: run::CellMemo,
    pub(in crate::eval) admitted: Option<Vec<(usize, CellAddress)>>,
    pub(in crate::eval) memo_usage: run::MemoRetentionUsage,
    pub(in crate::eval) visiting: CellSet,
    pub(in crate::eval) names: HashSet<(usize, usize)>,
    pub(in crate::eval) numeric_date_effects:
        CellMap<poirot_functions::coercion::date_time::Effects>,
    pub(in crate::eval) budget: BudgetMeter,
    pub(in crate::eval) observation_state: execution_observation::ObservationState,
    pub(in crate::eval) consumer: Option<EvaluationTarget>,
    pub(in crate::eval) run_usage: RunUsage,
}

/// Execution storage owned by a run or borrowed from a persistent workspace.
pub(in crate::eval) enum Storage<'a> {
    Owned(Box<Runtime>),
    Borrowed(&'a mut Runtime),
}
impl std::ops::Deref for Storage<'_> {
    /// Persistent execution storage reached through this borrowed or owned port.
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        match self {
            Self::Owned(state) => state,
            Self::Borrowed(state) => state,
        }
    }
}
impl std::ops::DerefMut for Storage<'_> {
    fn deref_mut(&mut self) -> &mut Runtime {
        match self {
            Self::Owned(state) => state,
            Self::Borrowed(state) => state,
        }
    }
}
impl std::ops::Deref for Session<'_> {
    /// Persistent execution storage reached through this borrowed or owned port.
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        &self.runtime
    }
}
impl std::ops::DerefMut for Session<'_> {
    fn deref_mut(&mut self) -> &mut Runtime {
        &mut self.runtime
    }
}
impl Runtime {
    /// Create empty execution state around a validated cumulative work meter.
    pub(in crate::eval) fn new(budget: BudgetMeter) -> Self {
        Self {
            vm: Default::default(),
            lookup_prefixes: Default::default(),
            random_streams: Default::default(),
            extension_trace: Default::default(),
            preparation: Default::default(),
            cache: Default::default(),
            admitted: None,
            memo_usage: Default::default(),
            visiting: Default::default(),
            names: Default::default(),
            numeric_date_effects: Default::default(),
            budget,
            observation_state: Default::default(),
            consumer: None,
            run_usage: Default::default(),
        }
    }
}
