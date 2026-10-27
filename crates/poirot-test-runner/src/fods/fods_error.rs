/// Failure of native fixture corpus operations.
#[derive(Debug, thiserror::Error)]
pub enum FodsError {
    /// No fixtures match the selection.
    #[error("no fixtures match the selection")]
    EmptySelection,
    /// Standalone baseline must retain native sheet/cell bindings.
    #[error("standalone baseline must retain native sheet/cell bindings")]
    MissingNativeBindings,
    /// Manifest and baseline must match the pinned source revision.
    #[error("manifest and baseline must match the pinned source revision")]
    RevisionMismatch,
    /// Case count overflow.
    #[error("case count overflow")]
    CaseCountOverflow,
    /// Manifest case count does not equal fixture totals.
    #[error("manifest case count does not equal fixture totals")]
    ManifestCountMismatch,
    /// Duplicate baseline binding.
    #[error("duplicate baseline case or fixture/index binding")]
    DuplicateBaselineBinding,
    /// Baseline must bind every declared case exactly once.
    #[error("baseline must bind every declared case exactly once")]
    IncompleteBaseline,
    /// Cannot read source checkout revision.
    #[error("cannot read source checkout revision")]
    SourceRevisionUnavailable,
    /// Source formula fixtures must match the pinned checkout without local edits.
    #[error("source formula fixtures must match the pinned checkout without local edits")]
    SourceFixturesChanged,
    /// Refresh destination must not be a symlink.
    #[error("refresh destination must not be a symlink")]
    SymlinkDestination,
    /// Source contains no fods formula fixtures.
    #[error("source contains no FODS formula fixtures")]
    MissingSourceFixtures,
    /// Source case ids collide.
    #[error("source case IDs collide")]
    DuplicateSourceCase,
    /// Evaluator panic prevents publishing a refresh baseline.
    #[error("evaluator panic prevents publishing a refresh baseline")]
    RefreshEvaluatorPanicked,
    /// Missing source case during refresh.
    #[error("missing source case during refresh")]
    MissingRefreshCase,
    /// Native classify requires a standalone classifications.json; use baseline update for the combined checked-in corpus.
    #[error(
        "native classify requires a standalone classifications.json; use baseline update for the combined checked-in corpus"
    )]
    MissingStandaloneBaseline,
    /// Harness errors prevent gap classification.
    #[error("harness errors prevent gap classification")]
    ClassificationHarnessErrors,
    /// Classification baseline changed during the run.
    #[error("classification baseline changed during the run")]
    ClassificationBaselineChanged,
    /// Could not start fods workers.
    #[error("could not start FODS workers")]
    Parallel(#[from] rayon::ThreadPoolBuildError),
    /// Could not write fods report.
    #[error("could not write FODS report")]
    Output(#[from] std::io::Error),
    /// Could not encode fods report.
    #[error("could not encode FODS report")]
    JsonOutput(#[from] serde_json::Error),
    /// System clock precedes the unix epoch.
    #[error("system clock precedes the Unix epoch")]
    Clock(#[from] std::time::SystemTimeError),
    /// Path prefix.
    #[error("fixture path lies outside source root")]
    PathPrefix(#[from] std::path::StripPrefixError),
    /// Io.
    #[error("could not access FODS artifact {path:?}")]
    Io {
        /// Artifact path.
        path: std::path::PathBuf,
        /// Filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// Metadata.
    #[error("invalid FODS metadata {path:?}")]
    Metadata {
        /// Metadata path.
        path: std::path::PathBuf,
        /// Metadata encoding or decoding failure.
        #[source]
        source: serde_json::Error,
    },
    /// Fods.
    #[error("could not extract FODS fixture {path:?}")]
    Fods {
        /// Native fixture path.
        path: std::path::PathBuf,
        /// Source extraction failure.
        #[source]
        source: super::xml::ExtractionError,
    },
    /// Could not load combined corpus baseline.
    #[error("could not load combined corpus baseline")]
    CombinedBaseline {
        /// Combined baseline validation failure.
        #[source]
        source: Box<crate::RunnerError>,
    },
    /// Progress writer lock is poisoned.
    #[error("progress writer lock is poisoned")]
    PoisonedProgressWriter,
    /// Computed source cache.
    #[error("computed source caches must be blank for fixture {fixture:?}")]
    ComputedSourceCache {
        /// Fixture identity.
        fixture: String,
    },
    /// Invalid fixture path.
    #[error("invalid fixture path for {fixture:?}")]
    InvalidFixturePath {
        /// Fixture identity.
        fixture: String,
    },
    /// Invalid digest.
    #[error("invalid fixture SHA-256 for {fixture:?}")]
    InvalidDigest {
        /// Fixture identity.
        fixture: String,
    },
    /// Duplicate fixture.
    #[error("duplicate fixture {fixture:?}")]
    DuplicateFixture {
        /// Fixture identity.
        fixture: String,
    },
    /// Unknown fixture.
    #[error("baseline references unknown fixture {fixture:?}")]
    UnknownFixture {
        /// Fixture identity.
        fixture: String,
    },
    /// Missing baseline case.
    #[error("case {case:?} is missing from baseline")]
    MissingBaselineCase {
        /// Source case identity.
        case: String,
    },
    /// Baseline identity mismatch.
    #[error("baseline identity differs from native source case {case:?}")]
    BaselineIdentityMismatch {
        /// Source case identity.
        case: String,
    },
    /// Invalid case coordinates.
    #[error("invalid sheet or coordinates in case {case:?}")]
    InvalidCaseCoordinates {
        /// Source case identity.
        case: String,
    },
    /// Invalid baseline binding.
    #[error("invalid baseline binding {case:?}")]
    InvalidBaselineBinding {
        /// Source case identity.
        case: String,
    },
    /// Digest mismatch.
    #[error("fixture {fixture:?} SHA-256 differs from pinned manifest")]
    DigestMismatch {
        /// Fixture identity.
        fixture: String,
        /// Pinned SHA-256.
        expected: String,
        /// Observed SHA-256.
        actual: String,
    },
    /// Case count mismatch.
    #[error("fixture {fixture:?} case count {actual} differs from manifest {expected}")]
    CaseCountMismatch {
        /// Fixture identity.
        fixture: String,
        /// Pinned number of cases.
        expected: usize,
        /// Extracted number of cases.
        actual: usize,
    },
    /// Classification case missing.
    #[error("no classification result for case {case:?}")]
    ClassificationCaseMissing {
        /// Source case identity.
        case: String,
    },
    /// Source must be pinned at expected.
    #[error("source must be pinned at {expected}")]
    SourceRevisionMismatch {
        /// Pinned revision.
        expected: &'static str,
    },
    /// Unrelated destination content.
    #[error("refresh destination contains unrelated content {path:?}")]
    UnrelatedDestinationContent {
        /// Unexpected destination entry.
        path: std::path::PathBuf,
    },
    /// Missing destination parent.
    #[error("destination {path:?} has no parent")]
    MissingDestinationParent {
        /// Destination path.
        path: std::path::PathBuf,
    },
    /// Requested case is absent from the selected fixture.
    #[error("unknown case {case:?} in fixture {fixture:?}")]
    UnknownCase {
        /// Requested fixture identity.
        fixture: String,
        /// Requested case identity.
        case: String,
    },
}
