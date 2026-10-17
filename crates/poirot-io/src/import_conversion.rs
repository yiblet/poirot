//! Bounded file ingress and ordered XLSX/BIFF backend import execution.
use crate::decoded_cells::{fidelity_warnings, identifier_warnings, value};
use crate::{
    ImportReport, IoError, PreflightError, ReadOptions, ResourceBudget, biff_overlay,
    biff_preflight, ooxml, opc,
};
use calamine::{Reader, Xls, Xlsx};
use poirot_model::{Cell, CellAddress, CellValue, DateSystem, Workbook};
use std::{fs, io::Cursor, path::Path};

fn read_bytes(path: &Path, options: ReadOptions) -> Result<Vec<u8>, IoError> {
    use std::io::Read;
    if fs::metadata(path)?.len() > options.max_input_bytes {
        return Err(IoError::Limit(
            "input exceeds configured byte budget".into(),
        ));
    }
    let mut contents = Vec::new();
    fs::File::open(path)?
        .take(options.max_input_bytes.saturating_add(1))
        .read_to_end(&mut contents)?;
    if contents.len() as u64 > options.max_input_bytes {
        return Err(IoError::Limit(
            "input exceeds configured byte budget".into(),
        ));
    }
    Ok(contents)
}

/// Detect XLSX ZIP or XLS compound-file signatures, independent of extension.
/// For inspectable fidelity warnings use [`read_workbook_with_report`].
pub fn read_workbook(path: impl AsRef<Path>) -> Result<Workbook, IoError> {
    Ok(read_workbook_with_report(path)?.workbook)
}
/// Read by file signature and retain inspectable import fidelity warnings.
pub fn read_workbook_with_report(path: impl AsRef<Path>) -> Result<ImportReport, IoError> {
    read_workbook_with_options(path, ReadOptions::default())
}
/// Read by file signature under explicit input resource settings.
pub fn read_workbook_with_options(
    path: impl AsRef<Path>,
    options: ReadOptions,
) -> Result<ImportReport, IoError> {
    let bytes = read_bytes(path.as_ref(), options)?;
    if bytes.starts_with(b"PK\x03\x04") {
        read_xlsx_bytes_with_options(&bytes, options)
    } else if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        read_xls_bytes_with_options(&bytes, options)
    } else {
        Err(IoError::Unsupported(
            "input is neither an XLSX ZIP nor an XLS compound file".into(),
        ))
    }
}
/// Read an XLSX file using default limits and return its canonical workbook.
pub fn read_xlsx(path: impl AsRef<Path>) -> Result<Workbook, IoError> {
    Ok(read_xlsx_bytes(&read_bytes(path.as_ref(), ReadOptions::default())?)?.workbook)
}
/// Read an XLS file using default limits and return its canonical workbook.
pub fn read_xls(path: impl AsRef<Path>) -> Result<Workbook, IoError> {
    Ok(read_xls_bytes(&read_bytes(path.as_ref(), ReadOptions::default())?)?.workbook)
}

/// Decode XLSX bytes using default limits and retain warnings.
pub fn read_xlsx_bytes(bytes: &[u8]) -> Result<ImportReport, IoError> {
    read_xlsx_bytes_with_options(bytes, ReadOptions::default())
}
/// Decode XLSX bytes under explicit input resource settings.
pub fn read_xlsx_bytes_with_options(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<ImportReport, IoError> {
    read_xlsx_bytes_with_policy(bytes, options, opc::OpcImportPolicy::default())
}
/// Execute checked OPC planning and backend decoding with an explicit external-link policy.
pub fn read_xlsx_bytes_with_policy(
    bytes: &[u8],
    options: ReadOptions,
    policy: opc::OpcImportPolicy,
) -> Result<ImportReport, IoError> {
    let package = ooxml::read_package_with_options(bytes, options)?;
    let plan = opc::PackageInventory {
        parts: package.clone(),
    }
    .plan_with_policy(policy)?;
    let backend_bytes = ooxml::backend_package(&package, &plan)?;
    let mut reader =
        Xlsx::new(Cursor::new(backend_bytes)).map_err(|e| IoError::Read(e.to_string()))?;
    let mut workbook = Workbook::new();
    workbook.date_system = if reader.has_1904_epoch() {
        DateSystem::Excel1904
    } else {
        DateSystem::Excel1900
    };
    let expected: Vec<_> = plan.sheets.iter().map(|s| s.name.clone()).collect();
    if reader.sheet_names() != expected {
        return Err(IoError::Opc(opc::OpcError::SheetMismatch(
            "backend sheet inventory differs from checked OPC plan".into(),
        )));
    }
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let name = &identity.name;
        let sheet = workbook.import_sheet(name)?;
        let mut cells = reader
            .worksheet_cells_reader(name)
            .map_err(|e| IoError::Read(e.to_string()))?;
        while let Some(record) = cells
            .next_cell_with_formula()
            .map_err(|e| IoError::Read(e.to_string()))?
        {
            let address = CellAddress::new(record.pos.0, record.pos.1)?;
            let mut cell = Cell::new(value(record.value.into())?);
            cell.formula = record.formula;
            sheet.set(address, cell)?;
        }
    }
    let mut warnings = fidelity_warnings();
    warnings.extend(plan.warnings.iter().cloned());
    ooxml::apply_rows(&package, &plan, &mut workbook)?;
    ooxml::apply_text(&package, &plan, &mut workbook)?;
    ooxml::apply_metadata(&package, &plan, &mut workbook, &mut warnings)?;
    ooxml::apply_spills(&package, &plan, &mut workbook)?;
    identifier_warnings(&workbook, &mut warnings);
    Ok(ImportReport { workbook, warnings })
}

/// Decode XLS bytes using default limits and retain warnings.
pub fn read_xls_bytes(bytes: &[u8]) -> Result<ImportReport, IoError> {
    read_xls_bytes_with_options(bytes, ReadOptions::default())
}
/// Decode XLS bytes under explicit input resource settings.
pub fn read_xls_bytes_with_options(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<ImportReport, IoError> {
    read_xls_bytes_with_budget(bytes, options, ResourceBudget::default())
}
/// Inspect CFB/BIFF resource claims before executing the dense backend adapter.
/// A refusal reports an operational budget or representation failure, never a
/// spreadsheet formula error or a claim that Excel rejects the source workbook.
/// Internal preflight validates record counts, extents and payloads against
/// [`crate::ResourceBudget`] before passing admitted input to the backend.
pub fn read_xls_bytes_with_budget(
    bytes: &[u8],
    options: ReadOptions,
    budget: ResourceBudget,
) -> Result<ImportReport, IoError> {
    if bytes.len() as u64 > options.max_input_bytes {
        return Err(IoError::Limit(
            "input exceeds configured byte budget".into(),
        ));
    }
    let row_plan = biff_preflight::inspect_xls(bytes, budget)?;
    let overlay = biff_overlay::Overlay::read(bytes, budget)?;
    // Calamine still contains unchecked BIFF formula-token accesses. Keep their
    // failures operational rather than unwinding through the public import API.
    let mut reader = std::panic::catch_unwind(|| Xls::new(Cursor::new(&overlay.backend)))
        .map_err(|_| {
            IoError::BiffPreflight(PreflightError::Representation(
                "backend BIFF token decoder panicked",
            ))
        })?
        .map_err(|e| IoError::Read(e.to_string()))?;
    let mut workbook = Workbook::new();
    workbook.date_system = if reader.has_1904_epoch() {
        DateSystem::Excel1904
    } else {
        DateSystem::Excel1900
    };
    let mut identity_warnings = Vec::new();
    let mut identity_comparisons = 0u64;
    for (index, backend_name) in reader.sheet_names().into_iter().enumerate() {
        let name = overlay
            .sheet_names
            .get(index)
            .unwrap_or(&backend_name)
            .clone();
        for existing in workbook.sheets() {
            identity_comparisons += 1;
            if identity_comparisons > budget.max_records as u64 {
                return Err(PreflightError::Resource {
                    resource: "import identifier comparisons",
                    requested: identity_comparisons,
                    limit: budget.max_records as u64,
                }
                .into());
            }
            if let Err(error) = poirot_model::identifiers::checked_matches(existing.name(), &name) {
                identity_warnings.push(error.to_string());
            }
        }
        let range = reader
            .worksheet_range(&backend_name)
            .map_err(|e| IoError::Read(e.to_string()))?;
        let formulas = reader
            .worksheet_formula(&backend_name)
            .map_err(|e| IoError::Read(e.to_string()))?;
        let sheet = workbook.import_sheet(&name)?;
        if let Some(start) = range.start() {
            for (r, c, data) in range.used_cells() {
                sheet.set(
                    CellAddress::new(start.0 + r as u32, start.1 + c as u32)?,
                    Cell::new(value(data.clone())?),
                )?;
            }
        }
        if let Some(start) = formulas.start() {
            for (r, c, formula) in formulas.used_cells() {
                let address = CellAddress::new(start.0 + r as u32, start.1 + c as u32)?;
                let cached = sheet
                    .get(address)
                    .map_or(CellValue::Blank, |c| c.value.clone());
                sheet.set_formula(address, formula, cached)?;
            }
        }
    }
    for (index, rows) in row_plan.row_indices.iter().enumerate() {
        if let Some(sheet) = workbook.sheet_mut(index) {
            for &row in rows {
                sheet.create_row(row)?;
            }
        }
    }
    let mut warnings = fidelity_warnings();
    warnings.extend(identity_warnings);
    warnings.extend(overlay.warnings.iter().cloned());
    warnings.push(
        "XLS raw number formats and unsupported BIFF token families remain backend-limited".into(),
    );
    for (name, formula) in reader.defined_names() {
        // Keep backend-supported names without revalidating Excel's name grammar.
        workbook.named_ranges.push(poirot_model::NamedRange {
            name: name.clone(),
            formula: formula.trim_start_matches('=').into(),
            sheet_scope: None,
        });
    }
    overlay.apply(&mut workbook)?;
    for (index, name) in workbook.named_ranges.iter().enumerate() {
        // SAFETY: index comes from enumerating the same named_ranges vector, which is not mutated during this loop.
        for existing in &workbook.named_ranges[..index] {
            identity_comparisons += 1;
            if identity_comparisons > budget.max_records as u64 {
                return Err(PreflightError::Resource {
                    resource: "import identifier comparisons",
                    requested: identity_comparisons,
                    limit: budget.max_records as u64,
                }
                .into());
            }
            if existing.sheet_scope != name.sheet_scope {
                continue;
            }
            if let Err(error) =
                poirot_model::identifiers::checked_matches(&existing.name, &name.name)
            {
                warnings.push(error.to_string());
            }
        }
    }
    Ok(ImportReport { workbook, warnings })
}
