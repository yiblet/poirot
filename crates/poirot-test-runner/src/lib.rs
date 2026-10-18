//! Source-backed workbook tests, shared selection and reviewed baseline comparison.
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
