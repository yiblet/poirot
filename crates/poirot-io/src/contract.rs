//! File errors and options shared by workbook readers and writers.
use crate::opc;

/// File-size limits and format policies for reading a workbook.
///
/// ZIP limits apply only to XLSX. [`Self::xls_budget`] controls XLS decoding.
/// Defaults impose no application quotas; integer maxima mean unlimited for
/// direct field users. Named setters replace only one field. Resource ceilings
/// do not disable package validation or unsupported-feature checks.
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
