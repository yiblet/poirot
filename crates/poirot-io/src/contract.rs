//! File errors and options shared by workbook readers and writers.
use crate::{PreflightError, opc};
use poirot_model::ModelError;
use std::fmt;

/// An error reading or writing a workbook.
#[derive(Debug)]
pub enum IoError {
    /// Filesystem read/write failure.
    File(std::io::Error),
    /// Backend workbook-decoding failure.
    Read(String),
    /// Backend workbook-encoding failure.
    Write(rust_xlsxwriter::XlsxError),
    /// Decoded content cannot be represented by the workbook model.
    Model(ModelError),
    /// OOXML parsing or writing failure.
    Xml(String),
    /// Package identity, relationship, or namespace validation failure.
    Opc(opc::OpcError),
    /// XLS record validation or a resource check failed before decoding.
    BiffPreflight(PreflightError),
    /// Configured import/export resource limit exceeded.
    Limit(String),
    /// Requested file feature or representation is unsupported.
    Unsupported(String),
    /// Numeric representation would violate the selected fidelity policy.
    NumericFidelity(poirot_model::NumericFidelityRefusal),
}
impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(e) => write!(f, "file IO: {e}"),
            Self::Read(e) => write!(f, "workbook import: {e}"),
            Self::Write(e) => write!(f, "XLSX export: {e}"),
            Self::Model(e) => write!(f, "workbook model: {e}"),
            Self::Xml(e) => write!(f, "OOXML: {e}"),
            Self::Opc(e) => write!(f, "OPC: {e}"),
            Self::BiffPreflight(e) => write!(f, "XLS preflight: {e}"),
            Self::Limit(e) => write!(f, "resource limit: {e}"),
            Self::NumericFidelity(e) => write!(f, "{e}"),
            Self::Unsupported(e) => write!(f, "unsupported: {e}"),
        }
    }
}
impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NumericFidelity(e) => Some(e),
            Self::File(e) => Some(e),
            Self::Write(e) => Some(e),
            Self::Model(e) => Some(e),
            Self::Opc(e) => Some(e),
            Self::BiffPreflight(e) => Some(e),
            _ => None,
        }
    }
}
impl From<PreflightError> for IoError {
    fn from(e: PreflightError) -> Self {
        Self::BiffPreflight(e)
    }
}
impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        Self::File(e)
    }
}
impl From<ModelError> for IoError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
impl From<rust_xlsxwriter::XlsxError> for IoError {
    fn from(e: rust_xlsxwriter::XlsxError) -> Self {
        Self::Write(e)
    }
}

/// File-size limits and format policies for reading a workbook.
///
/// ZIP limits apply only to XLSX. [`Self::xls_budget`] controls XLS decoding.
#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    /// Maximum input bytes, from a file or buffer (default 32 MiB).
    pub max_input_bytes: u64,
    /// Maximum expanded bytes in one XLSX ZIP entry (default 32 MiB).
    pub max_zip_entry_bytes: u64,
    /// Maximum sum of expanded XLSX entry bytes (default 128 MiB).
    pub max_zip_total_bytes: u64,
    /// Maximum XLSX ZIP entry count (default 10,000).
    pub max_zip_entries: usize,
    /// XLSX package relationship and external-link policy.
    pub xlsx_policy: opc::OpcImportPolicy,
    /// XLS structural and decoded-resource limits.
    pub xls_budget: crate::ResourceBudget,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 32 * 1024 * 1024,
            max_zip_entry_bytes: 32 * 1024 * 1024,
            max_zip_total_bytes: 128 * 1024 * 1024,
            max_zip_entries: 10_000,
            xlsx_policy: opc::OpcImportPolicy::default(),
            xls_budget: crate::ResourceBudget::default(),
        }
    }
}
/// Size limits for writing XLSX files.
///
/// Package sizes are checked after the backend constructs the workbook and ZIP,
/// so these limits do not bound peak memory use. [`Self::max_source_bytes`] can
/// reject oversized source data before serializer cell structures are built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    /// Maximum final compressed XLSX package bytes (default 32 MiB).
    pub max_output_bytes: u64,
    /// Maximum expanded bytes in one generated ZIP entry (default 32 MiB).
    pub max_zip_entry_bytes: u64,
    /// Maximum sum of expanded generated entry bytes (default 128 MiB).
    pub max_zip_total_bytes: u64,
    /// Maximum generated ZIP entry count (default 10,000).
    pub max_zip_entries: usize,
    /// Maximum source payload bytes before serialization (unlimited by default).
    pub max_source_bytes: u64,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            max_output_bytes: 32 * 1024 * 1024,
            max_zip_entry_bytes: 32 * 1024 * 1024,
            max_zip_total_bytes: 128 * 1024 * 1024,
            max_zip_entries: 10_000,
            max_source_bytes: u64::MAX,
        }
    }
}
