//! Typed command-line interface for corpus execution and inspection.
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{num::NonZeroUsize, path::PathBuf};

/// Presentation format for corpus reports and inspection output.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable terminal output.
    #[default]
    Text,
    /// Typed machine-readable JSON records.
    Json,
}

/// Select bounded worker parallelism, honoring RAYON_NUM_THREADS when valid.
pub fn default_jobs() -> NonZeroUsize {
    // The environment setting is an advisory hint: absent, non-Unicode, zero,
    // malformed, and out-of-range hints deliberately select automatic concurrency.
    match std::env::var("RAYON_NUM_THREADS") {
        Ok(value) => {
            if let Ok(jobs) = value.parse::<NonZeroUsize>() {
                return jobs;
            }
        }
        Err(std::env::VarError::NotPresent | std::env::VarError::NotUnicode(_)) => {}
    }
    // The platform concurrency query is also advisory; one worker is the
    // specified fallback when the OS cannot provide a count.
    let available = match std::thread::available_parallelism() {
        Ok(available) => available,
        Err(_unavailable_count) => NonZeroUsize::MIN,
    };
    // SAFETY: available is nonzero and min with eight remains at least one.
    NonZeroUsize::new(available.get().min(8)).unwrap()
}

/// Top-level corpus-runner command line, defaulting to a normal run.
#[derive(Debug, Parser)]
#[command(
    name = "poirot-test-fods",
    about = "Run original LibreOffice Calc FODS formula checks",
    version,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    /// Optional explicit inspection or maintenance command.
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Default run arguments used when no subcommand is selected.
    #[command(flatten)]
    pub run: RunArgs,
}

/// Corpus execution, inspection, and baseline-maintenance operations.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Evaluate the selected corpus cases (the default command).
    Run(RunArgs),
    /// Show a parsed fixture or one of its source cases.
    Inspect(InspectArgs),
    /// Refresh the corpus from a pinned LibreOffice source checkout.
    Refresh(RefreshArgs),
    /// Label existing known gaps without changing the pass/fail baseline.
    Classify(ClassifyArgs),
}

/// Fixture selection, concurrency, and reporting options for evaluation.
#[derive(Clone, Debug, Args)]
pub struct RunArgs {
    /// Corpus directory containing manifest, classifications, and fixtures.
    #[arg(long, default_value_os_t = crate::fods::corpus::default_corpus())]
    pub corpus: PathBuf,
    /// Select fixture names containing this text.
    #[arg(long)]
    pub filter: Option<String>,
    /// Rayon workers (defaults to available threads, up to 8).
    #[arg(long, default_value_t = default_jobs())]
    pub jobs: NonZeroUsize,
    /// Fail for known gaps as well as regressions and harness errors.
    #[arg(long)]
    pub strict: bool,
    /// Output presentation; JSON emits one typed record per line.
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,
}

/// Source fixture/case selection for read-only inspection.
#[derive(Clone, Debug, Args)]
pub struct InspectArgs {
    /// Fixture name from the corpus manifest.
    pub fixture: String,
    /// Restrict output to this source case ID.
    #[arg(long)]
    pub case: Option<String>,
    /// Corpus root containing the fixture manifest.
    #[arg(long, default_value_os_t = crate::fods::corpus::default_corpus())]
    pub corpus: PathBuf,
    /// Presentation format for inspected source records.
    #[arg(long, value_enum, default_value = "text")]
    pub format: OutputFormat,
}

/// Pinned-source checkout and output corpus settings for regeneration.
#[derive(Clone, Debug, Args)]
pub struct RefreshArgs {
    /// Path to the pinned LibreOffice source checkout.
    #[arg(long)]
    pub source: PathBuf,
    /// Corpus root to refresh from the pinned source checkout.
    #[arg(long, default_value_os_t = crate::fods::corpus::default_corpus())]
    pub corpus: PathBuf,
    /// Worker count used during fixture extraction.
    #[arg(long, default_value_t = default_jobs())]
    pub jobs: NonZeroUsize,
}

/// Corpus and parallelism settings for labeling existing baseline gaps.
#[derive(Clone, Debug, Args)]
pub struct ClassifyArgs {
    /// Corpus root whose existing gaps will be classified.
    #[arg(long, default_value_os_t = crate::fods::corpus::default_corpus())]
    pub corpus: PathBuf,
    /// Worker count used during gap classification.
    #[arg(long, default_value_t = default_jobs())]
    pub jobs: NonZeroUsize,
}
