//! Immutable source inspection and borrowed XLSX export plans.
use crate::export_execution::serialize_xlsx;
use crate::{IoError, WriteOptions};
use poirot_model::{CellValue, Workbook};

/// Container produced by the XLSX export adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// OOXML spreadsheet package export.
    Xlsx,
}
/// How export treats saved formula results. `Preserve` writes their existing types
/// and values without recalculation or validation of their freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypedCachePolicy {
    /// Preserve saved formula result types rather than recalculating them.
    Preserve,
}
/// Features represented by this adapter; this does not promise opaque fidelity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportFeatures {
    /// Whether the source contains formula cells.
    pub formulas: bool,
    /// Whether saved formula results require typed cache encoding.
    pub typed_caches: bool,
    /// Whether the source contains defined names.
    pub names: bool,
    /// Whether source cells contain custom number formats.
    pub number_formats: bool,
    /// Whether ordinary typed cell visuals and workbook color context are serialized.
    pub visual_styles: bool,
}
/// Pure inspection of an immutable source. Its borrow prevents stale snapshots.
/// Source bytes count UTF-8 payloads for scalar text and two bytes per unit for raw UTF16 text, eight bytes per scalar and 24 bytes per
/// present row (a bounded row-record charge), not XML size or
/// peak memory. Exact ZIP expansion/output budgets are checked during execution.
#[derive(Debug)]
pub struct ExportPlan<'a> {
    workbook: &'a Workbook,
    options: WriteOptions,
    source_bytes: u64,
    cell_count: usize,
    row_count: usize,
}
impl<'a> ExportPlan<'a> {
    /// Return output resource settings captured during planning.
    pub fn options(&self) -> WriteOptions {
        self.options
    }
    /// Borrow the immutable workbook validated by this plan.
    pub fn source(&self) -> &'a Workbook {
        self.workbook
    }
    /// Return the planned source-size accounting.
    pub fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Return the number of stored source cells to encode.
    pub fn cell_count(&self) -> usize {
        self.cell_count
    }
    /// Return the number of explicit source rows to encode.
    pub fn row_count(&self) -> usize {
        self.row_count
    }
    /// Return the selected file representation.
    pub fn format(&self) -> ExportFormat {
        ExportFormat::Xlsx
    }
    /// Return the numeric fidelity contract checked during planning.
    pub fn numeric_representation_policy(&self) -> poirot_model::FiniteNumericRepresentationPolicy {
        crate::numeric_representation::POLICY
    }
    /// Return how saved formula values will be encoded.
    pub fn typed_cache_policy(&self) -> TypedCachePolicy {
        TypedCachePolicy::Preserve
    }
    /// Inspect supported source features selected for export.
    pub fn features(&self) -> ExportFeatures {
        ExportFeatures {
            formulas: true,
            typed_caches: true,
            names: true,
            number_formats: true,
            visual_styles: true,
        }
    }
    /// Serialize the validated source without mutating or recalculating it.
    pub fn execute(&self) -> Result<Vec<u8>, IoError> {
        serialize_xlsx(self.workbook, self.options)
    }
}
/// Validate source features and resource claims before XLSX serialization.
pub fn plan_xlsx_export(
    workbook: &Workbook,
    options: WriteOptions,
) -> Result<ExportPlan<'_>, IoError> {
    if workbook.sheets().is_empty() {
        return Err(IoError::Unsupported(
            "cannot export workbook without worksheets".into(),
        ));
    }
    crate::ooxml::validate_spills(workbook)?;
    let mut source_bytes = 0u64;
    let mut cell_count = 0usize;
    let mut row_count = 0usize;
    let mut charge = |bytes: usize| -> Result<(), IoError> {
        source_bytes = source_bytes
            .checked_add(bytes as u64)
            .ok_or_else(|| IoError::Limit("export source size overflow".into()))?;
        if source_bytes > options.max_source_bytes {
            return Err(IoError::Limit(format!(
                "export source payload {source_bytes} exceeds max_source_bytes {} before backend serialization",
                options.max_source_bytes
            )));
        }
        Ok(())
    };
    charge(
        workbook
            .style_context
            .retained_bytes()
            .ok_or_else(|| IoError::Limit("style context size overflow".into()))?,
    )?;
    for sheet in workbook.sheets() {
        let mut sheet_text_bytes = 0u64;
        charge(sheet.name().len())?;
        for _ in sheet.row_indices() {
            row_count = row_count
                .checked_add(1)
                .ok_or_else(|| IoError::Limit("export row count overflow".into()))?;
            charge(24)?;
        }
        for (address, cell) in sheet.cells() {
            address.validate()?;
            cell_count = cell_count
                .checked_add(1)
                .ok_or_else(|| IoError::Limit("export cell count overflow".into()))?;
            charge(match &cell.value {
                CellValue::Text(s) => s.len(),
                CellValue::TextUtf16(s) => s
                    .units()
                    .len()
                    .checked_mul(2)
                    .ok_or_else(|| IoError::Limit("UTF16 source size overflow".into()))?,
                _ => 8,
            })?;
            if cell.value.text_ref().is_some() {
                let storage_bytes = match &cell.value {
                    CellValue::Text(s) => s.len(),
                    CellValue::TextUtf16(s) => s
                        .units()
                        .len()
                        .checked_mul(2)
                        .ok_or_else(|| IoError::Limit("UTF16 source size overflow".into()))?,
                    _ => return Err(IoError::Unsupported("invalid text representation".into())),
                };
                sheet_text_bytes = sheet_text_bytes
                    .checked_add(storage_bytes as u64)
                    .ok_or_else(|| IoError::Limit("export worksheet payload overflow".into()))?;
                if sheet_text_bytes > options.max_zip_entry_bytes {
                    return Err(IoError::Limit(format!(
                        "worksheet {} text payload exceeds max_zip_entry_bytes {} before backend allocation",
                        sheet.name(),
                        options.max_zip_entry_bytes
                    )));
                }
            }
            if let Some(formula) = &cell.formula {
                charge(formula.len())?;
            }
            charge(
                cell.style
                    .text_bytes()
                    .ok_or_else(|| IoError::Limit("style size overflow".into()))?,
            )?;
            crate::numeric_representation::check(sheet.name(), *address, cell)?;
        }
    }
    for name in &workbook.named_ranges {
        charge(name.name.len())?;
        charge(name.formula.len())?;
    }
    Ok(ExportPlan {
        workbook,
        options,
        source_bytes,
        cell_count,
        row_count,
    })
}
