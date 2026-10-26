//! Bounded OOXML adapter facade and ordered export patch coordination.
use crate::data::{SheetSource, WorkbookSource};
mod backend;
mod budget;
pub(crate) use backend::{backend_package, needs_backend_adapter};
mod dynamic;
mod metadata;
mod styles;
pub(crate) use styles::{StyleExport, read_styles};
mod package;
pub(crate) use metadata::apply_workbook_metadata;
mod shared;
mod spill;
mod text;
mod text_overlay;
mod workbook;
mod worksheet;
pub(crate) use spill::{apply_sheet_spills, validate_spills};
mod xml;
/// Checked OOXML package overlays used around backend conversion.
pub(crate) use text_overlay::{apply_sheet_text, shared_strings};

/// Checked OOXML package overlays used around backend conversion.
pub(crate) use package::ImportPackage;

use crate::{IoError, ReadOptions};
use budget::{SizeCounter, next_total};
use package::read_package_with_options;
use workbook::WorkbookPatch;
use worksheet::SheetPatch;
use xml::part;

/// Restore canonical source metadata and typed caches into backend-generated OOXML.
pub(crate) fn patch_export(
    bytes: &[u8],
    workbook: &impl WorkbookSource,
    options: crate::WriteOptions,
    styles: &StyleExport<'_>,
) -> Result<Vec<u8>, IoError> {
    package::check_generated_claims(bytes, options)?;
    let package = read_package_with_options(
        bytes,
        ReadOptions {
            max_input_bytes: options.max_output_bytes,
            max_zip_entry_bytes: options.max_zip_entry_bytes,
            max_zip_total_bytes: options.max_zip_total_bytes,
            max_zip_entries: options.max_zip_entries,
            ..ReadOptions::default()
        },
    )
    .map_err(|error| match error {
        // Generated-package validation retains the existing export error contract.
        IoError::Quota(failure) => IoError::Limit(format!(
            "generated package {:?} limit {} exceeded (consumed {}, requested {})",
            failure.resource, failure.limit, failure.consumed, failure.requested
        )),
        other => other,
    })?;
    let inventory = crate::opc::PackageInventory { parts: package };
    let plan = inventory.plan()?;
    let mut package = inventory.parts;
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
        let shared = shared::SharedExport::new(sheet)?;
        let patch = SheetPatch {
            bytes: original,
            sheet,
            shared: &shared,
            styles,
        };
        let size = patch.execute(SizeCounter::default())?.0;
        patched_total = next_total(size, original.len() as u64, patched_total, options)?;
        let updated = patch.execute(Vec::new())?;
        package.insert(path, updated);
    }
    let path = plan
        .styles_part
        .as_ref()
        .ok_or_else(|| IoError::Read("generated package has no styles part".into()))?;
    let original = part(&package, path)?;
    let size = styles
        .execute(SizeCounter::default(), workbook.style_context())?
        .0;
    patched_total = next_total(size, original.len() as u64, patched_total, options)?;
    package.insert(
        path.clone(),
        styles.execute(Vec::new(), workbook.style_context())?,
    );
    styles::write::install_theme(
        &mut package,
        &plan.workbook_part,
        plan.theme_part.as_deref(),
        workbook.style_context(),
        options,
        &mut patched_total,
    )?;
    let original = part(&package, &plan.workbook_part)?;
    let patch = WorkbookPatch {
        bytes: original,
        workbook,
    };
    let size = patch.execute(SizeCounter::default())?.0;
    next_total(size, original.len() as u64, patched_total, options)?;
    let updated = patch.execute(Vec::new())?;
    package.insert(plan.workbook_part, updated);
    if workbook.sheets().iter().any(|sheet| {
        sheet.spill_extents().next().is_some()
            || sheet
                .cells()
                .into_iter()
                .any(|(address, _)| sheet.spill_owner(*address).is_some())
    }) {
        dynamic::add_metadata(&mut package)?;
    }
    package::write_export(package, options)
}

// Stored spellings for modern reference syntax.
pub(crate) mod formula;
