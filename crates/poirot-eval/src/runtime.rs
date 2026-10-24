//! Persistent execution storage, independent of temporarily attached workbook ports.
use super::*;

/// Stacks, caches, budgets and effects retained across cell and output calls.
pub(crate) struct Runtime {
    pub(crate) vm: crate::bytecode_vm::Machine,
    pub(crate) lookup_prefixes: adapters::search::execution::LookupPrefixes,
    pub(crate) random_streams: CellMap<(random::Stream, usize)>,
    pub(crate) extension_trace: extensions::ExtensionTrace,
    pub(crate) preparation: spill_publication::Preparation,
    pub(crate) spill_read_ranges: CellMap<Vec<Area>>,
    pub(crate) blank_reads: CellMap<CellSet>,
    pub(crate) cache: run::CellMemo,
    pub(crate) admitted: Option<Vec<(usize, CellAddress)>>,
    pub(crate) memo_usage: run::MemoRetentionUsage,
    pub(crate) visiting: CellSet,
    pub(crate) names: HashSet<(usize, usize)>,
    pub(crate) numeric_date_effects: CellMap<poirot_coercionv2::date_time::Effects>,
    pub(crate) budget: BudgetMeter,
    pub(crate) observation_state: execution_observation::ObservationState,
    pub(crate) consumer: Option<EvaluationTarget>,
    pub(crate) run_usage: RunUsage,
}

/// Execution storage owned by a run or borrowed from a persistent workspace.
pub(crate) enum Storage<'a> {
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
    pub(crate) fn new(budget: BudgetMeter) -> Self {
        Self {
            vm: Default::default(),
            lookup_prefixes: Default::default(),
            random_streams: Default::default(),
            extension_trace: Default::default(),
            preparation: Default::default(),
            spill_read_ranges: Default::default(),
            blank_reads: Default::default(),
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
