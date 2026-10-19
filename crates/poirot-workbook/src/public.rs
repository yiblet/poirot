//! Loss-aware workbook loading and explicit, borrowed save planning.
//!
//! [`Workbook`] retains import warnings while exposing the model directly.
//! Saving preserves typed saved formula values and never recalculates. Such values
//! are file evidence, not verified evaluator cache provenance.
use crate::{io, model, xls};

use std::{fmt, path::Path};

/// Typed adapter failures retain their underlying error chain.
#[derive(Debug)]
pub enum WorkbookError {
    /// Import or XLSX export failure from the shared I/O layer.
    ImportOrXlsx(io::IoError),
    /// Checked BIFF export failure.
    Xls(xls::XlsError),
    /// Filesystem failure while publishing completed output bytes.
    File(std::io::Error),
}
impl fmt::Display for WorkbookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImportOrXlsx(e) => e.fmt(f),
            Self::Xls(e) => e.fmt(f),
            Self::File(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for WorkbookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::ImportOrXlsx(e) => e,
            Self::Xls(e) => e,
            Self::File(e) => e,
        })
    }
}
impl From<io::IoError> for WorkbookError {
    fn from(error: io::IoError) -> Self {
        Self::ImportOrXlsx(error)
    }
}
impl From<xls::XlsError> for WorkbookError {
    fn from(error: xls::XlsError) -> Self {
        Self::Xls(error)
    }
}

/// Workbook state with discoverable, retained import fidelity warnings.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Workbook {
    model: model::Workbook,
}
impl Workbook {
    /// Create an empty workbook with no import warnings.
    pub fn new() -> Self {
        Self::default()
    }
    /// Open an XLS or XLSX file with the default bounded import policies.
    /// File signatures select the format; diagnostics remain in model file metadata.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WorkbookError> {
        Self::load().open(path)
    }
    /// Import XLS or XLSX bytes with default bounded policies.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WorkbookError> {
        Self::load().from_bytes(bytes)
    }
    /// Wrap editable source without inventing file provenance.
    pub fn from_model(model: model::Workbook) -> Self {
        Self { model }
    }
    /// Borrow the engine's workbook source.
    pub fn model(&self) -> &model::Workbook {
        &self.model
    }
    /// Mutate engine source directly when advanced integration requires it.
    pub fn model_mut(&mut self) -> &mut model::Workbook {
        &mut self.model
    }
    /// Transfer the canonical source into a lower-level engine API.
    pub fn into_model(self) -> model::Workbook {
        self.model
    }
    /// Configure bounded spreadsheet import with format-specific policies.
    pub fn load() -> LoadBuilder {
        LoadBuilder::default()
    }
    /// Keep imported source and its typed fidelity diagnostics together.
    pub fn from_import(report: io::ImportReport) -> Self {
        Self {
            model: report.workbook,
        }
    }
    /// Configure native persistence of source and retained file metadata.
    pub fn save_snapshot(&self) -> crate::eval::artifact::SnapshotSaveBuilder<'_> {
        crate::eval::artifact::SnapshotSaveBuilder::workbook(&self.model)
    }
    /// Configure bounded native source or checkpoint loading.
    pub fn load_snapshot() -> crate::eval::artifact::SnapshotLoadBuilder {
        crate::eval::artifact::SnapshotLoadBuilder::default()
    }
    /// Restore canonical source from a native buffer, discarding optional preparation.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, crate::eval::artifact::SnapshotError> {
        let restored = Self::load_snapshot()
            .prepared_policy(crate::eval::artifact::PreparedPolicy::SourceOnly)
            .from_bytes(bytes)?;
        Ok(Self::from_model(restored.workbook))
    }
    /// Restore canonical source from a native file, discarding optional preparation.
    pub fn open_snapshot(
        path: impl AsRef<Path>,
    ) -> Result<Self, crate::eval::artifact::SnapshotError> {
        let restored = Self::load_snapshot()
            .prepared_policy(crate::eval::artifact::PreparedPolicy::SourceOnly)
            .open(path)?;
        Ok(Self::from_model(restored.workbook))
    }
    /// Configure an XLSX save with default options. Saving never recalculates.
    pub fn save_xlsx(&self) -> XlsxSaveBuilder<'_> {
        XlsxSaveBuilder {
            workbook: &self.model,
            options: io::WriteOptions::default(),
        }
    }
    /// Configure a BIFF8 XLS save with the default export policy.
    pub fn save_xls(&self) -> XlsSaveBuilder<'_> {
        XlsSaveBuilder {
            workbook: &self.model,
            policy: xls::XlsExportPolicy::default(),
        }
    }
}

/// Configure an XLSX save before validating or writing the borrowed workbook.
#[derive(Debug)]
#[must_use = "Call build, to_buffer, or write_file to save the workbook"]
pub struct XlsxSaveBuilder<'a> {
    workbook: &'a model::Workbook,
    options: io::WriteOptions,
}
impl<'a> XlsxSaveBuilder<'a> {
    /// Replace the default resource and output options.
    pub fn options(mut self, options: io::WriteOptions) -> Self {
        self.options = options;
        self
    }
    /// Validate the workbook without serializing or writing a file.
    pub fn build(self) -> Result<SavePlan<'a>, WorkbookError> {
        Ok(SavePlan::Xlsx(io::plan_xlsx_export(
            self.workbook,
            self.options,
        )?))
    }
    /// Validate and serialize the workbook to an owned buffer.
    pub fn to_buffer(self) -> Result<Vec<u8>, WorkbookError> {
        self.build()?.to_buffer()
    }
    /// Validate and serialize before opening the destination file.
    pub fn write_file(self, path: impl AsRef<Path>) -> Result<(), WorkbookError> {
        self.build()?.write_file(path)
    }
}

/// Configure a BIFF8 XLS save before validating or writing the borrowed workbook.
#[derive(Debug)]
#[must_use = "Call build, to_buffer, or write_file to save the workbook"]
pub struct XlsSaveBuilder<'a> {
    workbook: &'a model::Workbook,
    policy: xls::XlsExportPolicy,
}
impl<'a> XlsSaveBuilder<'a> {
    /// Replace the default representation and resource policy.
    pub fn policy(mut self, policy: xls::XlsExportPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// Validate the workbook without serializing or writing a file.
    pub fn build(self) -> Result<SavePlan<'a>, WorkbookError> {
        Ok(SavePlan::Xls(xls::XlsExportPlan::new(
            self.workbook,
            self.policy,
        )?))
    }
    /// Validate and serialize the workbook to an owned buffer.
    pub fn to_buffer(self) -> Result<Vec<u8>, WorkbookError> {
        self.build()?.to_buffer()
    }
    /// Validate and serialize before opening the destination file.
    pub fn write_file(self, path: impl AsRef<Path>) -> Result<(), WorkbookError> {
        self.build()?.write_file(path)
    }
}

/// A pure save plan retaining the source borrow and adapter-specific policies.
/// Planning performs no file writes and cannot mutate the workbook. Unsupported
/// XLS formulas fail during XLS planning, rather than becoming cached literals.
pub enum SavePlan<'a> {
    /// Checked immutable XLSX export plan.
    Xlsx(io::ExportPlan<'a>),
    /// Checked immutable XLS export plan.
    Xls(xls::XlsExportPlan<'a>),
}
impl fmt::Debug for SavePlan<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xlsx(_) => f.write_str("SavePlan::Xlsx"),
            Self::Xls(_) => f.write_str("SavePlan::Xls"),
        }
    }
}
impl SavePlan<'_> {
    /// Serialize the planned source; no recalculation or cache trust promotion.
    pub fn to_buffer(&self) -> Result<Vec<u8>, WorkbookError> {
        match self {
            Self::Xlsx(plan) => Ok(plan.execute()?),
            Self::Xls(plan) => Ok(plan.to_buffer()?),
        }
    }
    /// Serialization and resource checks finish before the destination is opened.
    /// This is a direct write, not an atomic filesystem replacement.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), WorkbookError> {
        let bytes = self.to_buffer()?;
        std::fs::write(path, bytes).map_err(WorkbookError::File)
    }
}

impl std::ops::Deref for Workbook {
    /// Canonical workbook source exposed for ordinary editing.
    type Target = model::Workbook;
    fn deref(&self) -> &Self::Target {
        &self.model
    }
}
impl std::ops::DerefMut for Workbook {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.model
    }
}
impl std::borrow::Borrow<model::Workbook> for Workbook {
    fn borrow(&self) -> &model::Workbook {
        &self.model
    }
}
impl std::borrow::Borrow<model::Workbook> for &Workbook {
    fn borrow(&self) -> &model::Workbook {
        &self.model
    }
}
impl From<model::Workbook> for Workbook {
    fn from(model: model::Workbook) -> Self {
        Self::from_model(model)
    }
}

/// Configure spreadsheet loading while keeping XLS and XLSX controls distinct.
#[derive(Debug, Default)]
#[must_use]
pub struct LoadBuilder {
    options: io::ReadOptions,
    xlsx_policy: io::opc::OpcImportPolicy,
    xls_budget: io::ResourceBudget,
}
impl LoadBuilder {
    /// Set common input and ZIP resource bounds.
    pub fn options(mut self, options: io::ReadOptions) -> Self {
        self.options = options;
        self
    }
    /// Set OOXML package and external-link admission policies.
    pub fn xlsx_policy(mut self, policy: io::opc::OpcImportPolicy) -> Self {
        self.xlsx_policy = policy;
        self
    }
    /// Set BIFF structural and decoded-resource bounds.
    pub fn xls_budget(mut self, budget: io::ResourceBudget) -> Self {
        self.xls_budget = budget;
        self
    }
    /// Read a bounded file and select its spreadsheet format by signature.
    pub fn open(self, path: impl AsRef<Path>) -> Result<Workbook, WorkbookError> {
        use std::io::Read;
        let file = std::fs::File::open(path).map_err(WorkbookError::File)?;
        let mut bytes = Vec::new();
        file.take(self.options.max_input_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(WorkbookError::File)?;
        self.from_bytes(&bytes)
    }
    /// Decode a bounded buffer and retain import diagnostics in its workbook.
    pub fn from_bytes(self, bytes: &[u8]) -> Result<Workbook, WorkbookError> {
        if bytes.len() as u64 > self.options.max_input_bytes {
            return Err(io::IoError::Limit("input file bytes".into()).into());
        }
        let report = if bytes.starts_with(b"PK\x03\x04") {
            io::read_xlsx_bytes_with_policy(bytes, self.options, self.xlsx_policy)?
        } else if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
            io::read_xls_bytes_with_budget(bytes, self.options, self.xls_budget)?
        } else {
            return Err(io::IoError::Unsupported("input is neither XLSX nor XLS".into()).into());
        };
        Ok(Workbook::from_import(report))
    }
}

impl std::borrow::BorrowMut<model::Workbook> for Workbook {
    fn borrow_mut(&mut self) -> &mut model::Workbook {
        &mut self.model
    }
}
