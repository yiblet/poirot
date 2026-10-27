//! Bounded file ingress and ordered XLSX/BIFF backend import execution.
use crate::data::{SheetSink, SheetSource, WorkbookSink};
use crate::decoded_cells::{fidelity_warnings, identifier_warnings};
use crate::xls::XlsWorkbook;
use crate::{Cell, CellAddress, DateSystem};
use crate::{FileFormat, FileMetadata, ImportDiagnostic, ImportDiagnosticKind};
use crate::{IoError, PreflightError, ReadOptions, ooxml};
use std::{fs, path::Path};

fn read_bytes(path: &Path, options: ReadOptions) -> Result<Vec<u8>, IoError> {
    use std::io::Read;
    let claimed = fs::metadata(path)?.len();
    if claimed > options.max_input_bytes {
        return Err(IoError::quota(
            crate::IoResource::InputBytes,
            options.max_input_bytes,
            0,
            claimed,
            path.display().to_string(),
        ));
    }
    let mut contents = Vec::new();
    fs::File::open(path)?
        .take(options.max_input_bytes.saturating_add(1))
        .read_to_end(&mut contents)?;
    if contents.len() as u64 > options.max_input_bytes {
        return Err(IoError::quota(
            crate::IoResource::InputBytes,
            options.max_input_bytes,
            0,
            contents.len() as u64,
            path.display().to_string(),
        ));
    }
    Ok(contents)
}

/// Read an XLS or XLSX file, retaining import warnings in its file metadata.
pub fn read_workbook<W: WorkbookSink<Sheet: SheetSink>>(
    path: impl AsRef<Path>,
) -> Result<W, IoError> {
    read_workbook_with_options::<W>(path, ReadOptions::default())
}
/// Read a file by signature using the supplied import policies and limits.
pub fn read_workbook_with_options<W: WorkbookSink<Sheet: SheetSink>>(
    path: impl AsRef<Path>,
    options: ReadOptions,
) -> Result<W, IoError> {
    read_workbook_bytes_with_options::<W>(&read_bytes(path.as_ref(), options)?, options)
}
/// Decode XLS or XLSX bytes, retaining import warnings in its file metadata.
pub fn read_workbook_bytes<W: WorkbookSink<Sheet: SheetSink>>(bytes: &[u8]) -> Result<W, IoError> {
    read_workbook_bytes_with_options::<W>(bytes, ReadOptions::default())
}
/// Decode bytes by signature using the supplied import policies and limits.
pub fn read_workbook_bytes_with_options<W: WorkbookSink<Sheet: SheetSink>>(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<W, IoError> {
    if bytes.len() as u64 > options.max_input_bytes {
        return Err(IoError::quota(
            crate::IoResource::InputBytes,
            options.max_input_bytes,
            0,
            bytes.len() as u64,
            "input buffer",
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

fn read_xlsx<W: WorkbookSink<Sheet: SheetSink>>(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<W, IoError> {
    let mut package = ooxml::ImportPackage::new(bytes, options)?;
    let plan = package.plan(options.xlsx_policy)?;
    let mut warnings = fidelity_warnings(FileFormat::Xlsx);
    warnings.extend(plan.warnings.iter().cloned());
    let styles = plan
        .styles_part
        .as_ref()
        .map(|p| package.read(p))
        .transpose()?;
    let theme = plan
        .theme_part
        .as_ref()
        .map(|p| package.read(p))
        .transpose()?;
    let styles = ooxml::read_styles(styles.as_deref(), theme, &mut warnings)?;
    let adapted = if ooxml::needs_backend_adapter(&plan) {
        Some(ooxml::backend_package(&mut package, &plan)?)
    } else {
        None
    };
    let mut workbook = match decode_xlsx::<W>(adapted.as_deref().unwrap_or(bytes), &plan) {
        Err(XlsxDecodeError::NeedsAdapter) if adapted.is_none() => {
            let adapted = ooxml::backend_package(&mut package, &plan)?;
            decode_xlsx::<W>(&adapted, &plan).map_err(XlsxDecodeError::into_io)?
        }
        result => result.map_err(XlsxDecodeError::into_io)?,
    };
    drop(adapted);
    workbook.set_style_context(styles.context);
    ooxml::apply_workbook_metadata(
        &package.read(&plan.workbook_part)?,
        &plan,
        &mut workbook,
        &mut warnings,
    )?;
    let shared = {
        let bytes = plan
            .shared_strings_part
            .as_ref()
            .map(|path| package.read(path))
            .transpose()?;
        ooxml::shared_strings(bytes.as_deref())?
    };
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let bytes = package.read(&identity.part)?;
        let sheet = workbook
            .sheet_by_name_mut(&identity.name)
            .ok_or_else(|| IoError::Read("missing text worksheet".into()))?;
        if ooxml::apply_sheet_text(&bytes, &shared, sheet, &styles.cells, &mut warnings)? {
            ooxml::apply_sheet_spills(&bytes, &identity.name, &mut workbook)?;
        }
    }
    identifier_warnings(&workbook, &mut warnings);
    workbook.set_file_metadata(FileMetadata {
        format: FileFormat::Xlsx,
        import_warnings: warnings,
    });
    Ok(workbook)
}

/// Backend failures distinguish content needing transient adaptation from input errors.
enum XlsxDecodeError {
    NeedsAdapter,
    Input(IoError),
}
impl XlsxDecodeError {
    /// Convert a terminal backend failure to the public I/O error.
    fn into_io(self) -> IoError {
        match self {
            Self::NeedsAdapter => IoError::Read("unsupported cell content after adaptation".into()),
            Self::Input(error) => error,
        }
    }
}
impl From<IoError> for XlsxDecodeError {
    fn from(error: IoError) -> Self {
        Self::Input(error)
    }
}
impl From<crate::ModelError> for XlsxDecodeError {
    fn from(error: crate::ModelError) -> Self {
        Self::Input(error.into())
    }
}
/// Stream decoded cells directly into Poirot without allocating a second workbook.
fn decode_xlsx<W: WorkbookSink<Sheet: SheetSink>>(
    bytes: &[u8],
    plan: &crate::opc::OpcReadPlan,
) -> Result<W, XlsxDecodeError> {
    use calamine::{Reader, Xlsx, XlsxFormulaMetadata};
    let backend_error = |error: calamine::XlsxError| match &error {
        calamine::XlsxError::CellError(value) if matches!(value.as_str(), "#SPILL!" | "#CALC!") => {
            XlsxDecodeError::NeedsAdapter
        }
        calamine::XlsxError::UnexpectedNode("v, f, or is") => XlsxDecodeError::NeedsAdapter,
        _ => XlsxDecodeError::Input(IoError::Read(error.to_string())),
    };
    let mut reader = Xlsx::new(std::io::Cursor::new(bytes)).map_err(backend_error)?;
    let mut workbook = W::default();
    workbook.set_date_system(if reader.has_1904_epoch() {
        DateSystem::Excel1904
    } else {
        DateSystem::Excel1900
    });
    if reader
        .sheet_names()
        .iter()
        .map(String::as_str)
        .ne(plan.sheets.iter().map(|s| s.name.as_str()))
    {
        return Err(IoError::Read(
            "backend worksheet inventory differs from checked OPC plan".into(),
        )
        .into());
    }
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let sheet = workbook.import_sheet(&identity.name)?;
        let mut cells = reader
            .worksheet_cells_reader(&identity.name)
            .map_err(backend_error)?;
        let mut masters = std::collections::BTreeMap::new();
        let mut followers = Vec::new();
        // The metadata API avoids Calamine's dense si-indexed expansion table;
        // its convenience formula API also offsets from range.start instead of
        // the actual master coordinate. Resolve sparse followers below instead.
        while let Some(record) = cells
            .next_cell_with_formula_metadata()
            .map_err(backend_error)?
        {
            let address = CellAddress::new(record.pos.0, record.pos.1)?;
            let mut cell = Cell::new(crate::decoded_cells::xlsx_value(record.value.into())?);
            cell.formula = match record.formula {
                Some(XlsxFormulaMetadata::Normal { formula }) => Some(formula),
                Some(XlsxFormulaMetadata::Shared {
                    shared_index,
                    formula,
                    ..
                }) => {
                    masters.insert(shared_index, (record.pos, formula.clone()));
                    Some(formula)
                }
                Some(XlsxFormulaMetadata::SharedDerived { shared_index }) => {
                    followers.push((address, shared_index));
                    None
                }
                _ => None,
            };
            sheet.set(address, cell)?;
        }
        for (address, index) in followers {
            let (origin, source) = masters
                .get(&index)
                .ok_or_else(|| IoError::Read("shared formula has no master".into()))?;
            let formula =
                calamine::expand_shared_formula(source, *origin, (address.row, address.col))
                    .map_err(backend_error)?;
            let cell = sheet
                .get_mut(address)
                .ok_or_else(|| IoError::Read("missing shared follower".into()))?;
            cell.formula = Some(formula);
        }
    }
    Ok(workbook)
}

fn read_xls<W: WorkbookSink<Sheet: SheetSink>>(
    bytes: &[u8],
    options: ReadOptions,
) -> Result<W, IoError> {
    let budget = options.xls_budget;
    let xls = XlsWorkbook::read(bytes, budget)?;
    let mut workbook = W::default();
    workbook.set_date_system(if xls.date1904 {
        DateSystem::Excel1904
    } else {
        DateSystem::Excel1900
    });
    let mut identity_warnings = Vec::new();
    let mut identity_comparisons = 0u64;
    for name in &xls.sheet_names {
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
            if let Err(error) = crate::identifiers::checked_matches(existing.name(), name) {
                identity_warnings.push(ImportDiagnostic {
                    kind: ImportDiagnosticKind::IdentifierIdentity,
                    detail: error.to_string(),
                });
            }
        }
        workbook.import_sheet(name)?;
    }
    for (index, rows) in xls.rows.iter().enumerate() {
        if let Some(sheet) = workbook.sheet_mut(index) {
            for (&row, &visibility) in rows {
                sheet.create_row(row)?;
                if visibility != crate::RowVisibility::default() {
                    sheet.set_row_visibility(row, visibility)?;
                }
            }
        }
    }
    let mut warnings = fidelity_warnings(FileFormat::Xls);
    warnings.extend(identity_warnings);
    warnings.extend(xls.warnings.iter().cloned().map(|detail| ImportDiagnostic {
        kind: ImportDiagnosticKind::RepresentationLoss,
        detail,
    }));
    warnings.push(ImportDiagnostic {
        kind: ImportDiagnosticKind::RepresentationLoss,
        detail: "XLS raw number formats are not retained; unsupported BIFF token families keep saved values without formula source"
            .into(),
    });
    xls.apply(&mut workbook)?;
    for (index, name) in workbook.named_ranges().iter().enumerate() {
        // SAFETY: index comes from enumerating the same named_ranges vector, which is not mutated during this loop.
        for existing in &workbook.named_ranges()[..index] {
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
            if let Err(error) = crate::identifiers::checked_matches(&existing.name, &name.name) {
                warnings.push(ImportDiagnostic {
                    kind: ImportDiagnosticKind::IdentifierIdentity,
                    detail: error.to_string(),
                });
            }
        }
    }
    workbook.set_file_metadata(FileMetadata {
        format: FileFormat::Xls,
        import_warnings: warnings,
    });
    Ok(workbook)
}
