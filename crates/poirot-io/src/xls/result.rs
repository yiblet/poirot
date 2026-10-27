//! Result type shared by the XLS decoder.
use crate::xls::preflight::PreflightError;

/// BIFF decoding result sharing preflight structural/resource diagnostics.
pub(super) type Result<T> = std::result::Result<T, PreflightError>;
