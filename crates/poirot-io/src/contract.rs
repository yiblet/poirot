//! Shared import/export contracts and operational budget defaults.
use crate::{PreflightError, opc};
use poirot_model::{ModelError, Workbook};
use std::fmt;

/// Workbook import/export failure, including invalid input, resource limits, and fidelity refusal.
/// A successful import can still report unsupported features in [`ImportReport::warnings`].
#[derive(Debug)]
pub enum IoError {
    /// Filesystem read/write failure.
    File(std::io::Error),
    /// Backend workbook-decoding failure.
    Read(String),
    /// Backend workbook-encoding failure.
    Write(rust_xlsxwriter::XlsxError),
    /// Decoded content violates canonical model constraints.
    Model(ModelError),
    /// OOXML parsing or writing failure.
    Xml(String),
    /// Package identity, relationship, or namespace validation failure.
    Opc(opc::OpcError),
    /// Pre-decode XLS structural or resource refusal.
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

/// Fidelity issues are exposed instead of pretending charts/macros/styles survive.
#[derive(Debug)]
pub struct ImportReport {
    /// Canonical decoded workbook, including saved formula results.
    pub workbook: Workbook,
    /// Inspectable omitted or altered source features.
    pub warnings: Vec<String>,
}

/// Operational resource budgets, not a claim that Excel rejects larger files.
/// Increase explicitly for trusted large workbooks. These limits bound input/ZIP
/// expansion. XLS additionally uses [`crate::ResourceBudget`] before backend decoding.
/// Those controls admit record counts, declared extents and decoded payloads;
/// they are separate from a process-wide memory limit.
#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    /// Maximum input file bytes (default 32 MiB).
    pub max_input_bytes: u64,
    /// Maximum expanded bytes in one XLSX ZIP entry (default 32 MiB).
    pub max_zip_entry_bytes: u64,
    /// Maximum sum of expanded XLSX entry bytes (default 128 MiB).
    pub max_zip_total_bytes: u64,
    /// Maximum XLSX ZIP entry count (default 10,000).
    pub max_zip_entries: usize,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: 32 * 1024 * 1024,
            max_zip_entry_bytes: 32 * 1024 * 1024,
            max_zip_total_bytes: 128 * 1024 * 1024,
            max_zip_entries: 10_000,
        }
    }
}
/// Explicit XLSX writer policy. Defaults retain the former wrapper's 32 MiB
/// entry and compressed-package, 128 MiB expanded-package and 10,000-entry limits.
/// These are operational limits, independent of importer policy. The backend
/// materializes the entire workbook and ZIP before exact package sizes are known;
/// these limits are not a bound on resident memory.
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
    /// Bound source payload before constructing the serializer's cell structures.
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
