//! Run spreadsheet compatibility cases and compare results.
//!
//! The development CLI runs authored JSON scenarios and LibreOffice FODS fixtures
//! against reviewed baselines. It can also generate reports from saved results.
//!
//! Run from the repository root:
//!
//! ```sh
//! cargo run -p poirot-test-runner -- run --source json --filter lookup
//! cargo run -p poirot-test-runner -- run --source fods --group text
//! ```
//!
//! Use `list` to preview the selection. Add `--strict` to fail on unchanged known gaps
//! and skips as well as regressions. Normal runs do not update baselines. This crate
//! is not published.
//!
//! Known gaps remain compatibility bugs. Baselines distinguish them from new
//! regressions so a fix can be reviewed against established behavior. When adding a
//! case, record the expected Excel result and its evidence; a passing Poirot run
//! alone does not establish compatibility.
pub mod baseline;
pub mod cli;
pub mod fods;
pub mod json;
pub mod model;
pub mod report;
pub mod runner;
pub mod selection;
pub mod skips;
mod sources;

pub use cli::Cli;

/// Runner errors are operational failures, never spreadsheet error values.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

pub(crate) fn invalid(message: impl Into<String>) -> Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()).into()
}

/// Location of the checked-in combined corpus and baseline.
pub fn package_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
