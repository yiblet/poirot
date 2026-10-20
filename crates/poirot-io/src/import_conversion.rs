//! Bounded file ingress and ordered XLSX/BIFF backend import execution.
use crate::decoded_cells::{fidelity_warnings, identifier_warnings, value};
use crate::{IoError, PreflightError, ReadOptions, biff_overlay, biff_preflight, ooxml, opc};
use calamine::{Reader, Xls, Xlsx};
use poirot_model::{Cell, CellAddress, CellValue, DateSystem, Workbook};
use poirot_model::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
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

/// Read an XLS or XLSX file, retaining import warnings in its file metadata.
pub fn read_workbook(path: impl AsRef<Path>) -> Result<Workbook, IoError> {
    read_workbook_with_options(path, ReadOptions::default())
}
/// Read a file by signature using the supplied import policies and limits.
pub fn read_workbook_with_options(
    path: impl AsRef<Path>,
    options: ReadOptions,
) -> Result<Workbook, IoError> {
    read_workbook_bytes_with_options(&read_bytes(path.as_ref(), options)?, options)
}
/// Decode XLS or XLSX bytes, retaining import warnings in its file metadata.
pub fn read_workbook_bytes(bytes: &[u8]) -> Result<Workbook, IoError> {
    read_workbook_bytes_with_options(bytes, ReadOptions::default())
}
/// Decode bytes by signature using the supplied import policies and limits.
pub fn read_workbook_bytes_with_options(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<Workbook, IoError> {
    if bytes.len() as u64 > options.max_input_bytes {
        return Err(IoError::Limit(
            "input exceeds configured byte budget".into(),
        ));
    }
    if bytes.starts_with(b"PK\x03\x04") {
        read_xlsx(bytes, options)
    } else if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]) {
        read_xls(bytes, options)
    } else {
        Err(IoError::Unsupported(
            "input is neither an XLSX ZIP nor an XLS compound file".into(),
        ))
    }
}

fn read_xlsx(bytes: &[u8], options: ReadOptions) -> Result<Workbook, IoError> {
    let package = ooxml::read_package_with_options(bytes, options)?;
    let plan = opc::PackageInventory {
        parts: package.clone(),
    }
    .plan_with_policy(options.xlsx_policy)?;
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
    workbook.file_metadata = Some(FileMetadata {
        format: FileFormat::Xlsx,
        import_warnings: warnings,
    });
    Ok(workbook)
}

fn read_xls(bytes: &[u8], options: ReadOptions) -> Result<Workbook, IoError> {
    let budget = options.xls_budget;
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
                identity_warnings.push(ImportDiagnostic {
                    kind: ImportDiagnosticKind::IdentifierIdentity,
                    detail: error.to_string(),
                });
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
                sheet.set_formula_with_cached_value(address, formula, cached)?;
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
    warnings.extend(
        overlay
            .warnings
            .iter()
            .cloned()
            .map(|detail| ImportDiagnostic {
                kind: ImportDiagnosticKind::RepresentationLoss,
                detail,
            }),
    );
    warnings.push(ImportDiagnostic {
        kind: ImportDiagnosticKind::RepresentationLoss,
        detail: "XLS raw number formats and unsupported BIFF token families remain backend-limited"
            .into(),
    });
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
                warnings.push(ImportDiagnostic {
                    kind: ImportDiagnosticKind::IdentifierIdentity,
                    detail: error.to_string(),
                });
            }
        }
    }
    workbook.file_metadata = Some(FileMetadata {
        format: FileFormat::Xls,
        import_warnings: warnings,
    });
    Ok(workbook)
}
