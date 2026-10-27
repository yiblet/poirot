/// Failure of combined compatibility runner operations.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    /// Baseline must use schema 2 and the pinned fods source revision.
    #[error("baseline must use schema 2 and the pinned FODS source revision")]
    BaselineSchemaMismatch,
    /// Baseline updates require a completed, unfiltered run of both default corpora.
    #[error("baseline updates require a completed, unfiltered run of both default corpora")]
    IncompleteBaselineRun,
    /// Baseline updates need a review reason.
    #[error("baseline updates need a review reason")]
    MissingReviewReason,
    /// Harness errors and stale json annotations cannot be accepted into a baseline.
    #[error("harness errors and stale JSON annotations cannot be accepted into a baseline")]
    UnacceptableObservation,
    /// Duplicate case in report.
    #[error("duplicate case in report")]
    DuplicateReportCase,
    /// Baseline changed since this run; rerun before producing a candidate.
    #[error("baseline changed since this run; rerun before producing a candidate")]
    BaselineChanged,
    /// Extract to a separate corpus; the shared checked-in baseline is updated only through baseline update.
    #[error(
        "extract to a separate corpus; the shared checked-in baseline is updated only through baseline update"
    )]
    SharedCorpusExtraction,
    /// No tests match the selection; use list without filters to see available groups.
    #[error("no tests match the selection; use list without filters to see available groups")]
    EmptySelection,
    /// Unknown executed json scenario.
    #[error("unknown executed JSON scenario")]
    UnknownJsonScenario,
    /// Corpus roots must not be symlinks.
    #[error("corpus roots must not be symlinks")]
    SymlinkCorpus,
    /// Records after report summary.
    #[error("records after report summary")]
    RecordsAfterSummary,
    /// Duplicate or misplaced report header.
    #[error("duplicate or misplaced report header")]
    MisplacedHeader,
    /// Report must begin with a header.
    #[error("report must begin with a header")]
    MissingInitialHeader,
    /// Missing report header.
    #[error("missing report header")]
    MissingHeader,
    /// Incomplete report: missing final summary; baseline cannot be updated.
    #[error("incomplete report: missing final summary; baseline cannot be updated")]
    MissingSummary,
    /// Unsupported report schema.
    #[error("unsupported report schema")]
    UnsupportedReportSchema,
    /// Duplicate report inventory id.
    #[error("duplicate report inventory ID")]
    DuplicateInventoryId,
    /// Report case is duplicate, outside selection or inconsistent with inventory.
    #[error("report case is duplicate, outside selection or inconsistent with inventory")]
    InvalidReportCase,
    /// Report comparison is inconsistent with its baseline observation.
    #[error("report comparison is inconsistent with its baseline observation")]
    InvalidReportComparison,
    /// Report is missing selected cases or has invalid removals.
    #[error("report is missing selected cases or has invalid removals")]
    IncompleteReport,
    /// Report summary does not match its case records.
    #[error("report summary does not match its case records")]
    SummaryMismatch,
    /// Skip rules require a nonempty reason and a unique exact case id.
    #[error("skip rules require a nonempty reason and a unique exact case ID")]
    InvalidSkipRule,
    /// Runner artifact i/o failed.
    #[error("runner artifact I/O failed")]
    Io(#[from] std::io::Error),
    /// Runner metadata decoding or encoding failed.
    #[error("runner metadata decoding or encoding failed")]
    Json(#[from] serde_json::Error),
    /// Native fixture operation failed.
    #[error("native fixture operation failed")]
    Fods(#[from] crate::fods::FodsError),
    /// Authored scenario operation failed.
    #[error("authored scenario operation failed")]
    Scenario(#[from] crate::json::JsonError),
    /// Specification operation failed.
    #[error("specification operation failed")]
    Specification(#[from] crate::json::specs::SpecificationError),
    /// Runner input is not utf-8.
    #[error("runner input is not UTF-8")]
    Utf8(#[from] std::str::Utf8Error),
    /// Path prefix.
    #[error("runner path lies outside its corpus")]
    PathPrefix(#[from] std::path::StripPrefixError),
    /// Could not start runner workers.
    #[error("could not start runner workers")]
    Parallel(#[from] rayon::ThreadPoolBuildError),
    /// Suite evaluator panicked.
    #[error("suite evaluator panicked")]
    SuitePanicked,
    /// Unknown skip case.
    #[error("stale skip rule for unknown case {case}")]
    UnknownSkipCase {
        /// Case identity.
        case: String,
    },
    /// Unknown fods case.
    #[error("unknown executed FODS case {case}")]
    UnknownFodsCase {
        /// Case identity.
        case: String,
    },
    /// Duplicate test id case.
    #[error("duplicate test ID {case}")]
    DuplicateTestId {
        /// Case identity.
        case: String,
    },
    /// Case case is absent from saved report.
    #[error("case {case} is absent from saved report")]
    MissingSavedCase {
        /// Case identity.
        case: String,
    },
    /// Invalid baseline entry case.
    #[error("invalid baseline entry {case}")]
    InvalidBaselineEntry {
        /// Case identity.
        case: String,
    },
    /// Scenario file.
    #[error("could not load scenario {path:?}")]
    ScenarioFile {
        /// Requested scenario path.
        path: std::path::PathBuf,
        /// Scenario validation failure.
        #[source]
        source: crate::json::JsonError,
    },
    /// A failed candidate write could not remove its partial file.
    #[error("could not clean partial baseline candidate {path:?}: {cleanup}")]
    CandidateCleanup {
        /// Partial candidate path.
        path: std::path::PathBuf,
        /// Original candidate write failure.
        #[source]
        source: std::io::Error,
        /// Failure while removing the partial candidate.
        cleanup: std::io::Error,
    },
    /// The result consumer closed before workers delivered their results.
    #[error("runner result receiver closed")]
    ResultReceiverClosed,
}
