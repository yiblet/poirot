//! Loss-aware workbook loading and explicit, borrowed save planning.
//!
//! [`Spreadsheet`] retains import warnings while exposing the model directly.
//! Saving preserves typed saved formula values and never recalculates. Such values
//! are file evidence, not verified evaluator cache provenance.
use crate::{io, model::Workbook, xls};
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
#[derive(Debug, Default)]
pub struct Spreadsheet {
    workbook: Workbook,
    import_warnings: Vec<String>,
}
impl Spreadsheet {
    /// Create an empty workbook with no import warnings.
    pub fn new() -> Self {
        Self::default()
    }
    /// Open an XLS or XLSX file with the default bounded import policies.
    /// File signatures select the format; inspect [`Self::import_warnings`] afterwards.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WorkbookError> {
        Ok(Self::from_import(io::read_workbook_with_report(path)?))
    }
    /// Import XLS or XLSX bytes with the default bounded import policies.
    /// Use [`Self::load_bytes`] when resource or external-link policies must be explicit.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WorkbookError> {
        Self::load_bytes(
            bytes,
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }
    /// Wrap an existing editable model; no import diagnostics are invented.
    pub fn from_workbook(workbook: Workbook) -> Self {
        Self {
            workbook,
            import_warnings: Vec::new(),
        }
    }
    /// Keep both the imported model and its fidelity warnings.
    pub fn from_import(report: io::ImportReport) -> Self {
        Self {
            workbook: report.workbook,
            import_warnings: report.warnings,
        }
    }
    /// Signature selection is independent of a filename extension. Existing
    /// subsystem resource and external-link policies are passed through unchanged.
    pub fn load_bytes(
        bytes: &[u8],
        options: io::ReadOptions,
        xlsx_policy: io::opc::OpcImportPolicy,
        xls_budget: io::ResourceBudget,
    ) -> Result<Self, WorkbookError> {
        let report = if bytes.starts_with(b"PK\x03\x04") {
            io::read_xlsx_bytes_with_policy(bytes, options, xlsx_policy)?
        } else if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
            io::read_xls_bytes_with_budget(bytes, options, xls_budget)?
        } else {
            return Err(io::IoError::Unsupported("input is neither XLSX nor XLS".into()).into());
        };
        Ok(Self::from_import(report))
    }
    /// Inspect the canonical model, including current saved formula values.
    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }
    /// Edit the source; overlays borrow it immutably for their lifetime.
    pub fn workbook_mut(&mut self) -> &mut Workbook {
        &mut self.workbook
    }
    /// Features omitted or changed during import. Saving cannot recover these features.
    pub fn import_warnings(&self) -> &[String] {
        &self.import_warnings
    }
    /// Preserve warnings when transferring ownership back to the lower-level API.
    pub fn into_import_report(self) -> io::ImportReport {
        io::ImportReport {
            workbook: self.workbook,
            warnings: self.import_warnings,
        }
    }
    /// Validate and borrow the source for XLSX serialization without calculation.
    pub fn plan_xlsx_save(&self, options: io::WriteOptions) -> Result<SavePlan<'_>, WorkbookError> {
        Ok(SavePlan::Xlsx(io::plan_xlsx_export(
            &self.workbook,
            options,
        )?))
    }
    /// Validate and borrow the source for bounded BIFF8 XLS serialization.
    pub fn plan_xls_save(
        &self,
        policy: xls::XlsExportPolicy,
    ) -> Result<SavePlan<'_>, WorkbookError> {
        Ok(SavePlan::Xls(xls::XlsExportPlan::new(
            &self.workbook,
            policy,
        )?))
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
