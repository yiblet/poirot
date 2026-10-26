//! File errors and options shared by workbook readers and writers.
use crate::ModelError;
use crate::{PreflightError, opc};
use std::fmt;

/// Caller-controlled import resource, measured independently of heap usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoResource {
    /// Compressed source bytes for one load.
    InputBytes,
    /// Expanded bytes in an individual XLSX entry.
    ZipEntryBytes,
    /// Expanded bytes across one XLSX package.
    ZipTotalBytes,
    /// Entries declared in one XLSX package.
    ZipEntries,
}
/// Actual claim rejected before import under an explicit caller quota.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IoQuotaFailure {
    /// Rejected resource and unit.
    pub resource: IoResource,
    /// Configured maximum.
    pub limit: u64,
    /// Previously accepted amount when the resource is cumulative.
    pub consumed: u64,
    /// Additional amount requested by this operation.
    pub requested: u64,
    /// File, package entry, or import operation when known.
    pub operation: String,
}
/// An error reading or writing a workbook.
#[derive(Debug)]
pub enum IoError {
    /// Configured import quota with the actual rejected request.
    Quota(IoQuotaFailure),
    /// A decoded/package size cannot be represented independently of caller quotas.
    SizeOverflow(&'static str),
    /// A fallible file-buffer allocation failed, independently of configured quotas.
    Allocation(&'static str),
    /// Filesystem read/write failure.
    File(std::io::Error),
    /// Backend workbook-decoding failure.
    Read(String),
    /// Backend workbook-encoding failure.
    Write(rxls::WriteError),
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
    NumericFidelity(crate::NumericFidelityRefusal),
}
impl IoError {
    /// Describe a real rejected source claim without inventing backend consumption.
    pub(crate) fn quota(
        resource: IoResource,
        limit: u64,
        consumed: u64,
        requested: u64,
        operation: impl Into<String>,
    ) -> Self {
        Self::Quota(IoQuotaFailure {
            resource,
            limit,
            consumed,
            requested,
            operation: operation.into(),
        })
    }
}
impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Quota(e) => write!(
                f,
                "import {:?} quota {} exceeded (consumed {}, requested {}) during {}",
                e.resource, e.limit, e.consumed, e.requested, e.operation
            ),
            Self::SizeOverflow(operation) => write!(f, "size overflow during {operation}"),
            Self::Allocation(operation) => write!(f, "allocation refused during {operation}"),
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
        if e.kind() == std::io::ErrorKind::OutOfMemory {
            Self::Allocation("workbook file buffer")
        } else {
            Self::File(e)
        }
    }
}
impl From<ModelError> for IoError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
impl From<rxls::WriteError> for IoError {
    fn from(e: rxls::WriteError) -> Self {
        Self::Write(e)
    }
}

/// File-size limits and format policies for reading a workbook.
///
/// ZIP limits apply only to XLSX. [`Self::xls_budget`] controls XLS decoding.
/// Defaults impose no application quotas; integer maxima retain their historical
/// unlimited meaning for direct field users. Named setters replace only one field.
///
/// ```rust
/// use poirot_io::ReadOptions;
/// let options = ReadOptions::default().max_input_bytes(1_000_000).max_zip_entries(100);
/// assert_eq!(options.max_input_bytes, 1_000_000);
/// assert_eq!(options.max_zip_entries, 100);
/// // Named setters replace one quota and preserve the other selected policies.
/// assert_eq!(options.max_input_bytes(0).max_zip_entries, 100);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ReadOptions {
    /// Maximum input bytes, from a file or buffer (unlimited by default).
    pub max_input_bytes: u64,
    /// Maximum expanded bytes in one XLSX ZIP entry (unlimited by default).
    pub max_zip_entry_bytes: u64,
    /// Maximum sum of expanded XLSX entry bytes (unlimited by default).
    pub max_zip_total_bytes: u64,
    /// Maximum XLSX ZIP entry count (unlimited by default).
    pub max_zip_entries: usize,
    /// XLSX package relationship and external-link policy.
    pub xlsx_policy: opc::OpcImportPolicy,
    /// XLS structural and decoded-resource limits.
    pub xls_budget: crate::ResourceBudget,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            max_input_bytes: u64::MAX,
            max_zip_entry_bytes: u64::MAX,
            max_zip_total_bytes: u64::MAX,
            max_zip_entries: usize::MAX,
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
///
/// ```rust
/// use poirot_io::{IoError, WriteOptions, plan_xlsx_export};
/// use poirot_io::data::WorkbookSource;
///
/// fn limited_export<W: WorkbookSource>(book: &W) -> Result<Vec<u8>, IoError> {
///     let options = WriteOptions { max_source_bytes: 1_000_000, ..WriteOptions::default() };
///     // Source quotas are checked before constructing serializer cell structures.
///     plan_xlsx_export(book, options)?.execute()
/// }
/// ```
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
impl ReadOptions {
    /// Set the maximum input byte before decoding; zero admits none. Replaces only this setting.
    pub fn max_input_bytes(mut self, maximum: u64) -> Self {
        self.max_input_bytes = maximum;
        self
    }
    /// Remove the input byte quota. Structural and representation checks remain.
    pub fn without_input_bytes_limit(mut self) -> Self {
        self.max_input_bytes = u64::MAX;
        self
    }
    /// Inspect the input byte quota; `None` means no configured maximum.
    pub fn input_bytes_limit(&self) -> Option<u64> {
        (self.max_input_bytes != u64::MAX).then_some(self.max_input_bytes)
    }
    /// Set the maximum expanded bytes in each XLSX entry before decoding; zero admits none. Replaces only this setting.
    pub fn max_zip_entry_bytes(mut self, maximum: u64) -> Self {
        self.max_zip_entry_bytes = maximum;
        self
    }
    /// Remove the expanded bytes in each XLSX entry quota. Structural and representation checks remain.
    pub fn without_zip_entry_bytes_limit(mut self) -> Self {
        self.max_zip_entry_bytes = u64::MAX;
        self
    }
    /// Inspect the expanded bytes in each XLSX entry quota; `None` means no configured maximum.
    pub fn zip_entry_bytes_limit(&self) -> Option<u64> {
        (self.max_zip_entry_bytes != u64::MAX).then_some(self.max_zip_entry_bytes)
    }
    /// Set the maximum expanded bytes across one XLSX package before decoding; zero admits none. Replaces only this setting.
    pub fn max_zip_total_bytes(mut self, maximum: u64) -> Self {
        self.max_zip_total_bytes = maximum;
        self
    }
    /// Remove the expanded bytes across one XLSX package quota. Structural and representation checks remain.
    pub fn without_zip_total_bytes_limit(mut self) -> Self {
        self.max_zip_total_bytes = u64::MAX;
        self
    }
    /// Inspect the expanded bytes across one XLSX package quota; `None` means no configured maximum.
    pub fn zip_total_bytes_limit(&self) -> Option<u64> {
        (self.max_zip_total_bytes != u64::MAX).then_some(self.max_zip_total_bytes)
    }
    /// Set the maximum XLSX entries in one package before decoding; zero admits none. Replaces only this setting.
    pub fn max_zip_entries(mut self, maximum: usize) -> Self {
        self.max_zip_entries = maximum;
        self
    }
    /// Remove the XLSX entries in one package quota. Structural and representation checks remain.
    pub fn without_zip_entries_limit(mut self) -> Self {
        self.max_zip_entries = usize::MAX;
        self
    }
    /// Inspect the XLSX entries in one package quota; `None` means no configured maximum.
    pub fn zip_entries_limit(&self) -> Option<usize> {
        (self.max_zip_entries != usize::MAX).then_some(self.max_zip_entries)
    }
}
