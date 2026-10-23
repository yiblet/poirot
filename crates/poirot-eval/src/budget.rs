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
