//! Workbook files, editing, and formula calculation.
//!
//! Reading a formula cell returns its stored result. Call `recalculate` after
//! changing inputs to update dependent cells. Saving does not calculate formulas.
use crate::{io, model};

use std::{fmt, path::Path};

/// A failure to load or save a workbook.
#[derive(Debug)]
pub enum WorkbookError {
    /// Import or XLSX export failure from the shared I/O layer.
    ImportOrXlsx(io::IoError),
    /// Filesystem failure while reading or writing a workbook.
    File(std::io::Error),
}
impl fmt::Display for WorkbookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ImportOrXlsx(e) => e.fmt(f),
            Self::File(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for WorkbookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::ImportOrXlsx(e) => e,
            Self::File(e) => e,
        })
    }
}
impl From<io::IoError> for WorkbookError {
    fn from(error: io::IoError) -> Self {
        match error {
            io::IoError::File(error) => Self::File(error),
            error => Self::ImportOrXlsx(error),
        }
    }
}

/// An error while recalculating a workbook.
#[derive(Debug)]
pub enum CalculationError {
    /// Calculation settings or workbook preparation were rejected.
    Preparation(crate::OverlayError),
    /// Calculation stopped before all requested results were available.
    Evaluation(crate::EvaluationFailure),
    /// Calculated results could not be applied to the workbook.
    Model(model::ModelError),
}
impl fmt::Display for CalculationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preparation(error) => error.fmt(f),
            Self::Evaluation(error) => error.fmt(f),
            Self::Model(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for CalculationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Preparation(error) => error,
            Self::Evaluation(error) => error,
            Self::Model(error) => error,
        })
    }
}

/// An editable workbook with file loading, calculation, and saving.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Workbook {
    model: model::Workbook,
}
impl Workbook {
    /// Create an empty workbook.
    pub fn new() -> Self {
        Self::default()
    }
    /// Open an XLS or XLSX file, detecting its format from the contents.
    ///
    /// Import warnings are kept in the workbook's file metadata.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, WorkbookError> {
        Self::load().open(path)
    }
    /// Read an XLS or XLSX workbook from memory.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WorkbookError> {
        Self::load().from_bytes(bytes)
    }
    /// Wrap existing workbook data without loading a file.
    pub fn from_model(model: model::Workbook) -> Self {
        Self { model }
    }
    /// Borrow the workbook data.
    pub fn model(&self) -> &model::Workbook {
        &self.model
    }
    /// Mutably borrow the workbook data.
    pub fn model_mut(&mut self) -> &mut model::Workbook {
        &mut self.model
    }
    /// Take ownership of the workbook data.
    pub fn into_model(self) -> model::Workbook {
        self.model
    }
    /// Configure XLS or XLSX loading.
    pub fn load() -> LoadBuilder {
        LoadBuilder::default()
    }
    /// Configure selected-output calculation with one captured default context.
    pub fn evaluation(&self) -> crate::EvaluationBuilder<'_> {
        crate::EvaluationBuilder::new(self)
    }
    /// Configure incremental scenarios that preserve unaffected results across edits.
    pub fn overlay(&self) -> crate::OverlayBuilder<'_> {
        crate::OverlayBuilder::new(self)
    }
    /// Configure usable snapshot restoration with compatible reuse or default source recovery.
    /// Advanced artifact loading remains available through `load_snapshot`.
    pub fn restore_snapshot() -> crate::WorkbookSnapshotLoadBuilder {
        crate::WorkbookSnapshotLoadBuilder::default()
    }
    /// Inspect authored content and saved values without calculating.
    /// Resolve `sheet_by_name` first when missing sheets must differ from unstored cells.
    pub fn stored_cell(&self, sheet: &str, address: model::CellAddress) -> Option<&model::Cell> {
        self.model.sheet_by_name(sheet)?.get(address)
    }
    /// Read a stored cell from a named sheet without calculating its formula.
    ///
    /// Returns `None` if the sheet or cell is absent. An explicitly stored blank
    /// is a cell and returns `Some`. Sheet names use the same comparison as
    /// [`model::Workbook::sheet_by_name`]; pass the name without formula quoting.
    pub fn cell(&self, sheet: &str, address: model::CellAddress) -> Option<&model::Cell> {
        self.model.sheet_by_name(sheet)?.get(address)
    }
    /// Calculate the workbook and update its stored cell values.
    ///
    /// Use [`crate::Evaluation`] to query selected outputs with scenario inputs,
    /// or [`crate::Overlay`] to reuse unaffected calculations across edits.
    ///
    /// If calculation fails, none of the calculated values are written. Edits made
    /// before this call remain. Spreadsheet errors such as `#DIV/0!` are stored as
    /// cell values and do not make this method return an error.
    pub fn recalculate(&mut self) -> Result<(), CalculationError> {
        self.recalculate_with_context(crate::EvaluationContext::default())
    }
    /// Calculate the workbook with a supplied clock, random seed, locale, and limits.
    ///
    /// This has the same result and failure behavior as [`Self::recalculate`].
    pub fn recalculate_with_context(
        &mut self,
        context: crate::EvaluationContext,
    ) -> Result<(), CalculationError> {
        self.recalculate_with_policy(context, crate::EvaluationPolicy::default())
    }
    /// Recalculate using explicit producer, array, and compatibility policies.
    /// Caller-trusted producers consume saved results and reject incomplete saved arrays.
    /// All cache writes are atomic; input edits remain if evaluation fails.
    pub fn recalculate_with_policy(
        &mut self,
        context: crate::EvaluationContext,
        policy: crate::EvaluationPolicy,
    ) -> Result<(), CalculationError> {
        let changes = {
            let mut evaluation = crate::eval::overlay::Evaluation::with_workbook(&self.model)
                .with_policy(policy)
                .build_calculation(context)
                .map_err(CalculationError::Preparation)?;
            evaluation.all().map_err(CalculationError::Evaluation)?;
            evaluation.into_changes()
        };
        self.model.apply(changes).map_err(CalculationError::Model)
    }
    /// Rename a sheet and update references in cell formulas and defined names.
    /// Unsupported formulas or invalid names leave the workbook unchanged.
    pub fn rename_sheet(
        &mut self,
        sheet: usize,
        name: impl Into<String>,
    ) -> Result<model::EditOutcome, crate::edit::EditError> {
        let plan = crate::edit::EditPlanner::rename_sheet(
            &self.model,
            sheet,
            name,
            crate::edit::EditPolicy::default(),
        )?;
        self.model
            .apply_rename_sheet_plan(&plan)
            .map_err(crate::edit::EditError::Model)
    }
    /// Configure a native Poirot snapshot of this workbook.
    pub fn save_snapshot(&self) -> crate::eval::artifact::SnapshotSaveBuilder<'_> {
        crate::eval::artifact::SnapshotSaveBuilder::workbook(&self.model)
    }
    /// Configure loading a native workbook snapshot or calculation checkpoint.
    pub fn load_snapshot() -> crate::eval::artifact::SnapshotLoadBuilder {
        crate::eval::artifact::SnapshotLoadBuilder::default()
    }
    /// Restore workbook data from a native snapshot in memory.
    ///
    /// Restores workbook data only. Use [`Self::load_snapshot`] to restore
    /// optional calculation state as well.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, crate::eval::artifact::SnapshotError> {
        let restored = Self::load_snapshot()
            .prepared_policy(crate::eval::artifact::PreparedPolicy::SourceOnly)
            .from_bytes(bytes)?;
        Ok(Self::from_model(restored.workbook))
    }
    /// Restore workbook data from a native snapshot file.
    ///
    /// Restores workbook data only. Use [`Self::load_snapshot`] to restore
    /// optional calculation state as well.
    pub fn open_snapshot(
        path: impl AsRef<Path>,
    ) -> Result<Self, crate::eval::artifact::SnapshotError> {
        let restored = Self::load_snapshot()
            .prepared_policy(crate::eval::artifact::PreparedPolicy::SourceOnly)
            .open(path)?;
        Ok(Self::from_model(restored.workbook))
    }
    /// Configure an XLSX save. Saving does not recalculate formulas.
    pub fn save_xlsx(&self) -> XlsxSaveBuilder<'_> {
        XlsxSaveBuilder {
            workbook: &self.model,
            options: io::WriteOptions::default(),
        }
    }
}

/// Options for saving a workbook as XLSX.
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
        Ok(SavePlan {
            plan: io::plan_xlsx_export(self.workbook, self.options)?,
        })
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

/// A workbook validated for XLSX output without mutating its source.
#[derive(Debug)]
pub struct SavePlan<'a> {
    plan: io::ExportPlan<'a>,
}
impl SavePlan<'_> {
    /// Serialize the validated workbook without recalculating formulas.
    pub fn to_buffer(&self) -> Result<Vec<u8>, WorkbookError> {
        Ok(self.plan.execute()?)
    }
    /// Write the serialized workbook to a file.
    ///
    /// Serialization and resource checks finish before the destination is opened.
    /// A filesystem error can still leave a partial file; this is not an atomic
    /// file replacement.
    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), WorkbookError> {
        let bytes = self.to_buffer()?;
        std::fs::write(path, bytes).map_err(WorkbookError::File)
    }
}

impl std::ops::Deref for Workbook {
    /// Workbook data available for direct editing.
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

/// Options for loading XLS or XLSX workbooks.
///
/// [`Self::options`] replaces all settings. Format-specific setters replace only
/// their own settings, so the order of calls matters.
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
    pub fn open(self, path: impl AsRef<Path>) -> Result<Workbook, WorkbookError> {
        Ok(Workbook::from_model(io::read_workbook_with_options(
            path,
            self.options,
        )?))
    }
    /// Read XLS or XLSX bytes and keep import warnings in file metadata.
    pub fn from_bytes(self, bytes: &[u8]) -> Result<Workbook, WorkbookError> {
        Ok(Workbook::from_model(io::read_workbook_bytes_with_options(
            bytes,
            self.options,
        )?))
    }
}

impl std::borrow::BorrowMut<model::Workbook> for Workbook {
    fn borrow_mut(&mut self) -> &mut model::Workbook {
        &mut self.model
    }
}
