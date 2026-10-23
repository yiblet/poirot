//! Operational limits and accounting, separate from spreadsheet error values.
use poirot_model::CellAddress;

/// Limits on calculation work and per-operation storage.
///
/// Defaults are unlimited except for nesting depth, which is capped at 128.
/// `u64::MAX` and `usize::MAX` disable the corresponding limits; usage counters
/// saturate at those maxima. Work measures operations and visited data, not time.
/// Some kernels charge an estimated upper bound before running unmetered code.
///
/// `cell_work` counts work in one formula, excluding its dependency formulas.
/// Collection limits count elements in one collection, not total heap allocation.
/// Text limits count bytes in one variable buffer: UTF-8 bytes or two bytes per
/// UTF-16 unit. Fixed-size kernel scratch, including one run-local ordered
/// reduction state, is outside these limits.
///
/// Reused results still pass collection and text checks. Memo storage, tracing,
/// parsing, formatting, and file codecs have separate limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationBudgets {
    /// Maximum work charged across the whole evaluation.
    pub run_work: u64,
    /// Maximum direct work charged to one formula frame.
    pub cell_work: u64,
    /// Maximum nesting depth, subject to the engine's hard ceiling.
    pub depth: usize,
    /// Maximum logical elements admitted for any single collection.
    pub materialized_values: usize,
    /// Maximum states admitted for one wildcard match.
    pub wildcard_states: u64,
    /// Maximum byte storage admitted for one text buffer.
    pub text_bytes: usize,
}
impl Default for EvaluationBudgets {
    fn default() -> Self {
        Self {
            run_work: u64::MAX,
            cell_work: u64::MAX,
            depth: 128,
            materialized_values: usize::MAX,
            wildcard_states: u64::MAX,
            text_bytes: usize::MAX,
        }
    }
}
/// Reusable, opt-in calculation quotas, independent of profiling and semantic context.
///
/// Work is cumulative until an explicit new cycle; formula work excludes dependencies.
/// Collection values and text bytes are checked before each logical allocation or cached
/// result admission, not against total heap usage. No quotas are configured by default.
/// Nesting retains the engine's 128-frame safety ceiling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvaluationLimits {
    work: Option<u64>,
    formula_work: Option<u64>,
    collection_values: Option<usize>,
    text_bytes: Option<usize>,
    wildcard_states: Option<u64>,
    depth: Option<usize>,
}
impl EvaluationLimits {
    /// Set cumulative work units per explicit cycle; zero allows none. Checked before the operation.
    pub fn max_work(mut self, maximum: u64) -> Self {
        self.work = Some(maximum);
        self
    }
    /// Remove the work quota. Does not reset an existing cycle.
    pub fn without_work_limit(mut self) -> Self {
        self.work = None;
        self
    }
    /// Inspect cumulative work units per explicit cycle; `None` means no application quota.
    pub fn work_limit(&self) -> Option<u64> {
        self.work
    }
    /// Set direct work units per formula; zero allows none. Checked before the operation.
    pub fn max_formula_work(mut self, maximum: u64) -> Self {
        self.formula_work = Some(maximum);
        self
    }
    /// Remove the formula work quota. Does not reset an existing cycle.
    pub fn without_formula_work_limit(mut self) -> Self {
        self.formula_work = None;
        self
    }
    /// Inspect direct work units per formula; `None` means no application quota.
    pub fn formula_work_limit(&self) -> Option<u64> {
        self.formula_work
    }
    /// Set values per logical collection; zero allows none. Checked before the operation.
    pub fn max_collection_values(mut self, maximum: usize) -> Self {
        self.collection_values = Some(maximum);
        self
    }
    /// Remove the collection values quota. Does not reset an existing cycle.
    pub fn without_collection_values_limit(mut self) -> Self {
        self.collection_values = None;
        self
    }
    /// Inspect values per logical collection; `None` means no application quota.
    pub fn collection_values_limit(&self) -> Option<usize> {
        self.collection_values
    }
    /// Set bytes per logical text buffer; zero allows none. Checked before the operation.
    pub fn max_text_bytes(mut self, maximum: usize) -> Self {
        self.text_bytes = Some(maximum);
        self
    }
    /// Remove the text bytes quota. Does not reset an existing cycle.
    pub fn without_text_bytes_limit(mut self) -> Self {
        self.text_bytes = None;
        self
    }
    /// Inspect bytes per logical text buffer; `None` means no application quota.
    pub fn text_bytes_limit(&self) -> Option<usize> {
        self.text_bytes
    }
    /// Set visited states per wildcard match; zero allows none. Checked before the operation.
    pub fn max_wildcard_states(mut self, maximum: u64) -> Self {
        self.wildcard_states = Some(maximum);
        self
    }
    /// Remove the wildcard states quota. Does not reset an existing cycle.
    pub fn without_wildcard_states_limit(mut self) -> Self {
        self.wildcard_states = None;
        self
    }
    /// Inspect visited states per wildcard match; `None` means no application quota.
    pub fn wildcard_states_limit(&self) -> Option<u64> {
        self.wildcard_states
    }
    /// Set nested evaluation frames, at most 128; zero allows none. Checked before the operation.
    pub fn max_depth(mut self, maximum: usize) -> Self {
        self.depth = Some(maximum);
        self
    }
    /// Remove the depth quota. Does not reset an existing cycle.
    pub fn without_depth_limit(mut self) -> Self {
        self.depth = None;
        self
    }
    /// Inspect nested evaluation frames, at most 128; `None` means no application quota.
    pub fn depth_limit(&self) -> Option<usize> {
        self.depth
    }
}
impl From<EvaluationLimits> for EvaluationBudgets {
    fn from(limits: EvaluationLimits) -> Self {
        Self {
            run_work: limits.work.unwrap_or(u64::MAX),
            cell_work: limits.formula_work.unwrap_or(u64::MAX),
            depth: limits.depth.unwrap_or(128),
            materialized_values: limits.collection_values.unwrap_or(usize::MAX),
            wildcard_states: limits.wildcard_states.unwrap_or(u64::MAX),
            text_bytes: limits.text_bytes.unwrap_or(usize::MAX),
        }
    }
}
impl From<EvaluationBudgets> for EvaluationLimits {
    fn from(budgets: EvaluationBudgets) -> Self {
        Self {
            work: (budgets.run_work != u64::MAX).then_some(budgets.run_work),
            formula_work: (budgets.cell_work != u64::MAX).then_some(budgets.cell_work),
            depth: (budgets.depth != 128).then_some(budgets.depth),
            collection_values: (budgets.materialized_values != usize::MAX)
                .then_some(budgets.materialized_values),
            text_bytes: (budgets.text_bytes != usize::MAX).then_some(budgets.text_bytes),
            wildcard_states: (budgets.wildcard_states != u64::MAX)
                .then_some(budgets.wildcard_states),
        }
    }
}
/// The resource category that rejected an evaluation request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetKind {
    /// Invalid or unsupported budget configuration.
    Configuration,
    /// Total run work limit.
    RunWork,
    /// Per-formula direct work limit.
    CellWork,
    /// Nesting depth limit.
    Depth,
    /// Per-collection element limit.
    Materialization,
    /// Per-match wildcard state limit.
    WildcardStates,
    /// Per-buffer text storage limit.
    TextBytes,
}
/// Category of deterministic evaluator work, used for accounting rather than timing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkKind {
    /// Expression admission.
    Expression,
    /// Workbook cell read.
    CellRead,
    /// Sparse range entry inspection.
    SparseScan,
    /// Name or sheet namespace entry inspection.
    NamespaceScan,
    /// Collection element materialization.
    Materialization,
    /// Value comparison.
    Comparison,
    /// Scalar arithmetic or logical operation.
    ScalarOperator,
    /// Work performed by a builtin helper.
    HelperOperation,
    /// Wildcard matching state.
    WildcardState,
    /// Text processing byte.
    TextByte,
    /// Calendar iteration step.
    CalendarStep,
    /// Pair of references inspected by a reference operator.
    ReferencePair,
}
/// A cell identified by a zero-based workbook sheet index and row/column address.
/// Resolve once with an evaluation's `target` method, including quoted sheet names.
/// Targets bind to that workbook's sheet topology; arbitrary structural edits can invalidate them.
/// A valid unstored coordinate evaluates as blank; a missing worksheet is a resolution error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvaluationTarget {
    /// Zero-based workbook sheet index.
    pub sheet: usize,
    /// Validated row and column coordinates.
    pub address: CellAddress,
}
/// The rejected charge, previously consumed amount, limit, and cell attribution.
/// `requested` is the additional charge that would exceed the limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetFailure {
    /// Resource category that rejected the charge.
    pub kind: BudgetKind,
    /// Configured maximum for that category.
    pub limit: u64,
    /// Amount already consumed before the rejected charge.
    pub consumed: u64,
    /// Additional amount requested by the rejected operation.
    pub requested: u64,
    /// Formula cell executing when the charge failed, when available.
    pub target: Option<EvaluationTarget>,
}
/// Stable identity for an explicitly unsupported execution capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CapabilityId {
    /// Formula syntax preparation and configured structural bounds.
    FormulaParsing,
    /// Optional observation or retained execution evidence.
    ExecutionObservation,
    /// Structural bytecode compilation before execution.
    FormulaCompilation,
    /// Identifier comparison outside the supported contract.
    IdentifierComparison,
    /// Unsupported builtin adapter behavior.
    FunctionAdapter,
    /// Unsupported numeric conversion behavior.
    NumericConversion,
    /// Unsupported date text parsing behavior.
    DateParsing,
    /// Unsupported value formatting behavior.
    Formatting,
    /// External data access.
    ExternalData,
    /// User-defined provider execution.
    UserDefinedFunction,
}
/// Operational cause distinct from a configured quota or unsupported spreadsheet feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceFailureKind {
    /// A checked size or work estimate cannot be represented on this platform.
    SizeOverflow,
    /// The allocator refused a fallible reservation.
    Allocation,
    /// A native backend stopped under its own execution or representation guards.
    BackendGuard,
}
/// A failure that stops calculation rather than producing a spreadsheet error value.
///
/// Includes exhausted limits, stale plans, and unsupported operations. Formula
/// error handlers such as `IFERROR` cannot catch these failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluationFailure {
    /// Setter publication requires the independently selected pinned computation policy.
    IncompatibleMaterializationPolicy,
    /// An upstream algorithm exception that stops calculation.
    KnownSourceException {
        /// Cell executing the exceptional source behavior.
        source: Option<EvaluationTarget>,
        /// Pinned source exception identity.
        exception: poirot_model::KnownSourceException,
    },
    /// A calculated value would exceed the session's cached-result storage limit.
    MemoRetentionLimit {
        /// Configured cell and byte retention limits.
        limits: crate::run::MemoRetentionLimits,
        /// Retention usage before the rejected admission.
        consumed: crate::run::MemoRetentionUsage,
        /// Additional retention requested.
        requested: crate::run::MemoRetentionUsage,
        /// Formula active when admission failed.
        source: Option<EvaluationTarget>,
    },
    /// Formula/name/sheet topology or preparation policy differs from the plan.
    StalePlan,
    /// A metered engine operation exceeded its configured limit.
    Budget(BudgetFailure),
    /// A representation, allocation, or backend failure with no invented usage counts.
    Resource {
        /// Actual failure category, independent of application quotas.
        kind: ResourceFailureKind,
        /// Rejecting subsystem.
        capability: CapabilityId,
        /// Active cell when known.
        source: Option<EvaluationTarget>,
        /// Operation or native backend diagnostic.
        detail: String,
    },
    /// A component exceeded a structural limit or overflowed a size estimate.
    ///
    /// Unlike [`Self::Budget`], this has no evaluator work charge to report.
    ResourceLimit {
        /// Capability whose internal resource limit was exceeded.
        capability: CapabilityId,
        /// Cell executing the operation, when available.
        source: Option<EvaluationTarget>,
        /// Explanation supplied by the rejecting component.
        detail: String,
    },
    /// Execution requested behavior outside an installed capability's contract.
    UnsupportedCapability {
        /// Capability required by the operation.
        capability: CapabilityId,
        /// Cell executing the operation, when available.
        source: Option<EvaluationTarget>,
        /// Explanation of the unsupported behavior.
        detail: String,
    },
}
impl EvaluationFailure {
    /// Return metered budget details when this failure is a budget rejection.
    pub fn budget(&self) -> Option<&BudgetFailure> {
        match self {
            Self::Budget(failure) => Some(failure),
            Self::IncompatibleMaterializationPolicy
            | Self::KnownSourceException { .. }
            | Self::UnsupportedCapability { .. }
            | Self::ResourceLimit { .. }
            | Self::Resource { .. }
            | Self::StalePlan
            | Self::MemoRetentionLimit { .. } => None,
        }
    }
    /// Attach the active formula's coordinates to failures that support attribution.
    pub(crate) fn set_target(&mut self, target: EvaluationTarget) {
        match self {
            Self::Budget(failure) => failure.target = Some(target),
            Self::KnownSourceException { source, .. }
            | Self::UnsupportedCapability { source, .. }
            | Self::ResourceLimit { source, .. }
            | Self::Resource { source, .. }
            | Self::MemoRetentionLimit { source, .. } => *source = Some(target),
            Self::IncompatibleMaterializationPolicy | Self::StalePlan => {}
        }
    }
}
impl std::fmt::Display for EvaluationFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleMaterializationPolicy => {
                f.write_str("pinned cell-setter materialization requires pinned numeric outcomes")
            }
            Self::KnownSourceException { exception, .. } => {
                write!(f, "known pinned source exception: {exception:?}")
            }
            Self::MemoRetentionLimit {
                limits,
                consumed,
                requested,
                ..
            } => write!(
                f,
                "memo retention limit {limits:?} exceeded (consumed {consumed:?}, requested {requested:?})"
            ),
            Self::StalePlan => f.write_str("evaluation plan source or policy is stale"),
            Self::Budget(failure) => write!(
                f,
                "evaluation {:?} limit {} exceeded (consumed {}, requested {})",
                failure.kind, failure.limit, failure.consumed, failure.requested
            ),
            Self::Resource {
                kind,
                capability,
                detail,
                ..
            } => write!(f, "evaluation {kind:?} in {capability:?}: {detail}"),
            Self::ResourceLimit {
                capability, detail, ..
            } => {
                write!(f, "evaluation resource limit in {capability:?}: {detail}")
            }
            Self::UnsupportedCapability {
                capability, detail, ..
            } => write!(
                f,
                "unsupported evaluation capability {capability:?}: {detail}"
            ),
        }
    }
}
impl std::error::Error for EvaluationFailure {}
/// Work counters from a completed or failed calculation.
///
/// These measure engine operations, not elapsed time or total heap allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvaluationUsage {
    /// Cell-local random stream accesses observed during execution.
    pub random_accesses: Vec<crate::random::RandomAccess>,
    /// Total charged work units.
    pub work_units: u64,
    /// Admitted expression nodes.
    pub expressions: u64,
    /// Cell read operations.
    pub cell_reads: u64,
    /// Reads served from an execution cache.
    pub cache_hits: u64,
    /// Sparse range entries inspected.
    pub sparse_entries: u64,
    /// Name or sheet namespace entries inspected.
    pub namespace_entries: u64,
    /// Logical values charged for materialization.
    pub materialized_values: u64,
    /// Charged comparisons.
    pub comparisons: u64,
    /// Charged scalar operators.
    pub scalar_operators: u64,
    /// Charged builtin helper operations.
    pub helper_operations: u64,
    /// Charged wildcard matching states.
    pub wildcard_states: u64,
    /// Charged text processing bytes.
    pub text_bytes: u64,
    /// Charged calendar steps.
    pub calendar_steps: u64,
    /// Charged reference pairs.
    pub reference_pairs: u64,
    /// Greatest observed nesting depth.
    pub maximum_depth: usize,
    /// Cells whose execution completed.
    pub completed_cells: u64,
}
/// Completion or operational failure of a calculation, with usage in either case.
/// `Complete` can contain a spreadsheet error value: it means the engine finished
/// evaluating, not that the formula returned a number or text successfully.
#[derive(Clone, Debug, PartialEq)]
pub enum EvaluationOutcome<T> {
    /// Evaluation finished, possibly returning semantic spreadsheet errors.
    Complete {
        /// Completed caller result.
        value: T,
        /// Work consumed by successful execution.
        usage: EvaluationUsage,
    },
    /// Evaluation stopped with an operational failure.
    Aborted {
        /// Failure that stopped execution.
        failure: EvaluationFailure,
        /// Work consumed before execution stopped.
        usage: EvaluationUsage,
    },
}
impl<T> EvaluationOutcome<T> {
    /// Return usage counters from either a completion or an abort.
    pub fn usage(&self) -> &EvaluationUsage {
        match self {
            Self::Complete { usage, .. } | Self::Aborted { usage, .. } => usage,
        }
    }
}
/// Internal result carrier that keeps operational failures separate from cell errors.
pub(crate) type EvalResult<T> = Result<T, EvaluationFailure>;
mod meter;
mod policy;
/// Mutable work accounting shared by nested evaluator operations.
pub(crate) use meter::BudgetMeter;
