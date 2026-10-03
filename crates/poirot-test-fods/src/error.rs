use std::{fmt, path::PathBuf};

/// Runner infrastructure failures distinguished from formula mismatches and known gaps.
#[derive(Debug)]
pub enum Error {
    /// Filesystem failure while loading or publishing corpus artifacts.
    Io {
        /// Artifact path involved in the I/O failure.
        path: PathBuf,
        /// Underlying filesystem diagnostic.
        source: std::io::Error,
    },
    /// Corpus JSON metadata cannot be decoded.
    Metadata {
        /// Metadata artifact that failed decoding.
        path: PathBuf,
        /// Underlying schema/JSON diagnostic.
        source: serde_json::Error,
    },
    /// Corpus inputs cannot form a consistent execution plan.
    InvalidPlan(String),
    /// Native fixture bytes do not match retained provenance.
    Integrity {
        /// Fixture whose integrity check failed.
        fixture: String,
        /// Explanation of the provenance mismatch.
        message: String,
    },
    /// Native FODS parsing or extraction failure.
    Fods {
        /// Native fixture path.
        path: PathBuf,
        /// Source extraction diagnostic.
        message: String,
    },
    /// Extracted fixture cannot form a supported canonical workbook.
    Workbook {
        /// Fixture whose workbook preparation failed.
        fixture: String,
        /// Model or fixture-context diagnostic.
        message: String,
    },
    /// Parallel worker setup or execution failed.
    Parallel(String),
    /// Report rendering or output publication failed.
    Output(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Metadata { path, source } => {
                write!(f, "invalid metadata {}: {source}", path.display())
            }
            Self::InvalidPlan(message) => write!(f, "invalid test plan: {message}"),
            Self::Integrity { fixture, message } => write!(f, "{fixture}: {message}"),
            Self::Fods { path, message } => write!(f, "{}: {message}", path.display()),
            Self::Workbook { fixture, message } => {
                write!(f, "{fixture}: workbook preparation failed: {message}")
            }
            Self::Parallel(message) => write!(f, "Rayon: {message}"),
            Self::Output(source) => write!(f, "cannot write report: {source}"),
        }
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Metadata { source, .. } => Some(source),
            Self::Output(source) => Some(source),
            _ => None,
        }
    }
}
