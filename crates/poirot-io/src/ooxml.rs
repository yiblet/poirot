//! Bounded OOXML adapter facade and ordered export patch coordination.
mod backend;
mod budget;
mod metadata;
mod package;
mod spill;
mod text;
mod text_overlay;
mod workbook;
mod worksheet;
pub(crate) use spill::bounds as spill_bounds;
pub(crate) use spill::{apply_spills, validate_spills};
mod xml;
/// Checked OOXML package overlays used around backend conversion.
pub(crate) use text_overlay::apply_text;

/// Checked OOXML package overlays used around backend conversion.
pub(crate) use backend::backend_package;
/// Checked OOXML package overlays used around backend conversion.
pub(crate) use metadata::{apply_metadata, apply_rows};
/// Checked OOXML package overlays used around backend conversion.
pub(crate) use package::read_package_with_options;

use crate::{IoError, ReadOptions};
use budget::{SizeCounter, next_total};
use poirot_model::Workbook;
use workbook::WorkbookPatch;
use worksheet::SheetPatch;
use xml::part;

/// Restore canonical source metadata and typed caches into backend-generated OOXML.
pub(crate) fn patch_export(
    bytes: &[u8],
    workbook: &Workbook,
    options: crate::WriteOptions,
) -> Result<Vec<u8>, IoError> {
    package::check_generated_claims(bytes, options)?;
    let mut package = read_package_with_options(
        bytes,
        ReadOptions {
            max_input_bytes: options.max_output_bytes,
            max_zip_entry_bytes: options.max_zip_entry_bytes,
            max_zip_total_bytes: options.max_zip_total_bytes,
            max_zip_entries: options.max_zip_entries,
            ..ReadOptions::default()
        },
    )?;
    let plan = crate::opc::PackageInventory {
        parts: package.clone(),
    }
    .plan()?;
    if plan.sheets.len() != workbook.sheets().len() || plan.sheets.iter().any(|s| !s.supported) {
        return Err(IoError::Opc(crate::opc::OpcError::SheetMismatch(
            "export sheet cardinality mismatch".into(),
        )));
    }
    let mut patched_total = package.values().map(|part| part.len() as u64).sum();
    for sheet in workbook.sheets() {
        let path = plan
            .sheets
            .iter()
            .find(|s| s.name == sheet.name())
            .ok_or_else(|| IoError::Opc(crate::opc::OpcError::SheetMismatch(sheet.name().into())))?
            .part
            .clone();
        let original = part(&package, &path)?;
        let patch = SheetPatch {
            bytes: original,
            sheet,
        };
        let size = patch.execute(SizeCounter::default())?.0;
        patched_total = next_total(size, original.len() as u64, patched_total, options)?;
        let updated = patch.execute(Vec::new())?;
        package.insert(path, updated);
    }
    let original = part(&package, &plan.workbook_part)?;
    let patch = WorkbookPatch {
        bytes: original,
        workbook,
    };
    let size = patch.execute(SizeCounter::default())?.0;
    next_total(size, original.len() as u64, patched_total, options)?;
    let updated = patch.execute(Vec::new())?;
    package.insert(plan.workbook_part, updated);
    package::write_export(package, options)
}
