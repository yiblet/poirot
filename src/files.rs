//! Excel loading, export, source metadata, and import diagnostics.
//!
//! ```
//! use poirot::{Workbook, files::ReadOptions};
//! let mut book = Workbook::new();
//! book.add_sheet("Data")?.set_value("A1", 42)?;
//! let bytes = book.save_xlsx().to_buffer()?;
//! let loaded = Workbook::load().options(ReadOptions::default()).from_bytes(&bytes)?;
//! assert!(loaded.stored_cell("Data", "A1")?.is_some());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use crate::model::{self, Workbook};
use poirot_io as io;
use std::{borrow::Cow, path::Path};

pub use crate::model::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
pub use crate::xlsx_save_error::XlsxSaveError;
pub use poirot_io::opc::{ExternalRelationshipPolicy, OpcError, OpcImportPolicy};
pub use poirot_io::{
    FiniteNumericRepresentationPolicy, IoError, IoQuotaError, IoResource, NumericFidelityError,
    NumericFidelityReason, NumericFileFormat, NumericLocation, PreflightError, ReadOptions,
    ResourceBudget, WriteOptions,
};

/// Options for loading XLS or XLSX workbooks.
///
/// [`Self::options`] replaces all settings. Format-specific setters replace only
/// their own settings, so the order of calls matters.
///
/// ```
/// use poirot::Workbook;
/// use poirot::files::ReadOptions;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?;
/// let bytes = book.save_xlsx().to_buffer()?;
/// let options = ReadOptions::default().max_input_bytes(u64::try_from(bytes.len())?);
/// let loaded = Workbook::load().options(options).from_bytes(&bytes)?;
/// drop(bytes); // Loaded workbook data does not borrow the input.
/// assert_eq!(loaded.sheets().len(), 1);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Default)]
#[must_use]
pub struct LoadBuilder {
    options: io::ReadOptions,
}

impl LoadBuilder {
    /// Set input bytes for one load. A later `options` call replaces all options.
    pub fn max_input_bytes(mut self, maximum: u64) -> Self {
        self.options = self.options.max_input_bytes(maximum);
        self
    }

    /// Remove the configured input bytes maximum.
    pub fn without_input_bytes_limit(mut self) -> Self {
        self.options = self.options.without_input_bytes_limit();
        self
    }

    /// Set expanded bytes per XLSX entry for one load. A later `options` call replaces all options.
    pub fn max_zip_entry_bytes(mut self, maximum: u64) -> Self {
        self.options = self.options.max_zip_entry_bytes(maximum);
        self
    }

    /// Remove the configured expanded bytes per XLSX entry maximum.
    pub fn without_zip_entry_bytes_limit(mut self) -> Self {
        self.options = self.options.without_zip_entry_bytes_limit();
        self
    }

    /// Set total expanded XLSX bytes for one load. A later `options` call replaces all options.
    pub fn max_zip_total_bytes(mut self, maximum: u64) -> Self {
        self.options = self.options.max_zip_total_bytes(maximum);
        self
    }

    /// Remove the configured total expanded XLSX bytes maximum.
    pub fn without_zip_total_bytes_limit(mut self) -> Self {
        self.options = self.options.without_zip_total_bytes_limit();
        self
    }

    /// Set XLSX entry count for one load. A later `options` call replaces all options.
    pub fn max_zip_entries(mut self, maximum: usize) -> Self {
        self.options = self.options.max_zip_entries(maximum);
        self
    }

    /// Remove the configured XLSX entry count maximum.
    pub fn without_zip_entries_limit(mut self) -> Self {
        self.options = self.options.without_zip_entries_limit();
        self
    }

    /// Inspect the reusable configuration without consuming the builder.
    pub fn read_options(&self) -> &io::ReadOptions {
        &self.options
    }

    /// Replace all import options, including format-specific policies and limits.
    pub fn options(mut self, options: io::ReadOptions) -> Self {
        self.options = options;
        self
    }

    /// Set which XLSX package content and external links may be imported.
    pub fn xlsx_policy(mut self, policy: io::opc::OpcImportPolicy) -> Self {
        self.options.xlsx_policy = policy;
        self
    }

    /// Set resource limits for decoding XLS records.
    pub fn xls_budget(mut self, budget: io::ResourceBudget) -> Self {
        self.options.xls_budget = budget;
        self
    }

    /// Open an XLS or XLSX file and keep its import warnings in file metadata.
    pub fn open(self, path: impl AsRef<Path>) -> Result<Workbook, io::IoError> {
        io::read_workbook_with_options::<Workbook>(path, self.options)
    }

    /// Read XLS or XLSX bytes into owned data and keep import warnings in file metadata.
    /// The returned workbook does not borrow the input buffer.
    pub fn from_bytes(self, bytes: &[u8]) -> Result<Workbook, io::IoError> {
        io::read_workbook_bytes_with_options::<Workbook>(bytes, self.options)
    }
}

/// Options for saving a workbook or captured scenario as XLSX.
///
/// [`Workbook::save_xlsx`](model::Workbook::save_xlsx) borrows the workbook.
/// Scenario saves own the effective state captured when `save_xlsx` is called,
/// so later edits and dropping the scenario do not affect their output.
/// Serialization never calculates formulas.
///
/// ```
/// use poirot::Workbook;
/// let mut book = Workbook::new();
/// book.add_sheet("Data")?.set_value("A1", 42.)?;
/// let bytes = book.save_xlsx().to_buffer()?;
/// let loaded = Workbook::from_bytes(&bytes)?;
/// assert_eq!(loaded.stored_cell("Data", "A1")?.ok_or("missing value")?.value,
///     poirot::model::CellValue::Number(42.));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
#[must_use = "Call to_buffer or write_file to save the workbook"]
pub struct XlsxSaveBuilder<'a> {
    workbook: Result<Cow<'a, model::Workbook>, XlsxSaveError>,
    options: io::WriteOptions,
}
impl<'a> XlsxSaveBuilder<'a> {
    /// Borrow the source without copying workbook data.
    pub(crate) fn borrowed(workbook: &'a model::Workbook) -> Self {
        Self {
            workbook: Ok(Cow::Borrowed(workbook)),
            options: io::WriteOptions::default(),
        }
    }

    /// Capture effective edits now, retaining any assembly failure until serialization.
    pub(crate) fn capture(
        workbook: &model::Workbook,
        changes: Vec<model::WorkbookChange>,
    ) -> XlsxSaveBuilder<'static> {
        XlsxSaveBuilder {
            workbook: workbook
                .with_changes(changes)
                .map(Cow::Owned)
                .map_err(XlsxSaveError::from),
            options: io::WriteOptions::default(),
        }
    }

    /// Replace the default resource and output options.
    pub fn options(mut self, options: io::WriteOptions) -> Self {
        self.options = options;
        self
    }

    /// Validate and serialize the workbook to an owned buffer.
    pub fn to_buffer(self) -> Result<Vec<u8>, XlsxSaveError> {
        let workbook = self.workbook?;
        Ok(io::plan_xlsx_export(workbook.as_ref(), self.options)?.execute()?)
    }

    /// Validate and serialize before opening the destination file.
    ///
    /// A filesystem error can still leave a partial file; this is not an atomic
    /// file replacement.
    pub fn write_file(self, path: impl AsRef<Path>) -> Result<(), XlsxSaveError> {
        let bytes = self.to_buffer()?;
        std::fs::write(path, bytes).map_err(|error| XlsxSaveError::Io(error.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::XlsxSaveBuilder;
    use super::XlsxSaveError;
    use crate::model::{CellAddress, CellValue, ModelError, Workbook, WorkbookChange};
    use std::borrow::Cow;

    #[test]
    fn normal_save_borrows_and_failed_capture_retains_the_model_cause() {
        let workbook = Workbook::new();
        let save = workbook.save_xlsx();
        assert!(
            matches!(save.workbook, Ok(Cow::Borrowed(source)) if std::ptr::eq(source, &workbook))
        );
        let save = XlsxSaveBuilder::capture(
            &workbook,
            vec![WorkbookChange::SetValue {
                sheet: 0,
                address: CellAddress::new(0, 0).unwrap(),
                value: CellValue::Number(1.),
            }],
        );
        let error = save.to_buffer().unwrap_err();
        assert!(matches!(&error, XlsxSaveError::Model(_)));
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<ModelError>()
                .is_some()
        );
    }
}
