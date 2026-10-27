//! XLSX serializer execution and write-after-success filesystem egress.
use crate::CellAddress;
use crate::data::{SheetSource, WorkbookSource};
use crate::{IoError, WriteOptions, ooxml, plan_xlsx_export};
use std::{borrow::Cow, collections::BTreeMap, fs, path::Path};

/// Writes XLSX with explicit writer budgets; no recalculation is performed.
pub fn write_xlsx_to_buffer_with_options(
    workbook: &impl WorkbookSource,
    options: WriteOptions,
) -> Result<Vec<u8>, IoError> {
    plan_xlsx_export(workbook, options)?.execute()
}
/// Uses the documented [`WriteOptions::default`] compatibility policy.
pub fn write_xlsx_to_buffer(workbook: &impl WorkbookSource) -> Result<Vec<u8>, IoError> {
    write_xlsx_to_buffer_with_options(workbook, WriteOptions::default())
}
/// Encode an immutable source workbook into an admitted in-memory XLSX package.
pub(super) fn serialize_xlsx(
    workbook: &impl WorkbookSource,
    options: WriteOptions,
) -> Result<Vec<u8>, IoError> {
    if !workbook
        .sheets()
        .iter()
        .any(|s| s.presentation().visibility == crate::SheetVisibility::Visible)
    {
        return Err(IoError::Unsupported(
            "cannot export workbook without a visible worksheet".into(),
        ));
    }
    serialize_fresh(workbook, options)
}

/// Author a canonical package from the modeled workbook data.
pub(crate) fn serialize_fresh(
    workbook: &impl WorkbookSource,
    options: WriteOptions,
) -> Result<Vec<u8>, IoError> {
    if workbook.sheets().is_empty() {
        return Err(IoError::Unsupported(
            "cannot export workbook without worksheets".into(),
        ));
    }
    let active = workbook
        .sheets()
        .iter()
        .position(|s| s.presentation().visibility == crate::SheetVisibility::Visible)
        .ok_or_else(|| {
            IoError::Unsupported("cannot export workbook without a visible worksheet".into())
        })?;
    let mut styles = ooxml::StyleExport::new(workbook.style_context())?;
    let mut writer = rust_xlsxwriter::Workbook::new();
    // Storage spellings differing from source text, computed once for both
    // projection passes; other formulas are written from their source.
    let mut stored_formulas = Vec::new();
    for (index, sheet) in workbook.sheets().iter().enumerate() {
        let out = writer.add_worksheet();
        out.set_name(sheet.name())?;
        if index == active {
            out.set_active(true);
            out.set_first_tab(true);
        }
        match sheet.presentation().visibility {
            crate::SheetVisibility::Visible => {}
            crate::SheetVisibility::Hidden => {
                out.set_hidden(true);
            }
            crate::SheetVisibility::VeryHidden => {
                out.set_very_hidden(true);
            }
        }
        if let Some([r, g, b]) = sheet.presentation().tab_color {
            out.set_tab_color(rust_xlsxwriter::Color::RGB(
                (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
            ));
        }
        let mut bounds: Option<(CellAddress, CellAddress)> = None;
        let mut rewritten_formulas = BTreeMap::new();
        for (address, cell) in sheet.cells() {
            address.validate()?;
            crate::numeric_representation::check(sheet.name(), *address, cell)?;
            styles.register(&cell.style)?;
            if let Some(formula) = &cell.formula
                && let Cow::Owned(rewritten) = ooxml::formula::stored_formula(formula)?
            {
                rewritten_formulas.insert(*address, rewritten);
            }
            bounds = Some(bounds.map_or((*address, *address), |(first, last)| {
                (
                    CellAddress {
                        row: first.row.min(address.row),
                        col: first.col.min(address.col),
                    },
                    CellAddress {
                        row: last.row.max(address.row),
                        col: last.col.max(address.col),
                    },
                )
            }));
        }
        // The XML projection replaces sheetData with canonical cells; the backend
        // only needs the used-range corners to author the worksheet dimension.
        if let Some((first, last)) = bounds {
            for corner in [first, last] {
                let col = u16::try_from(corner.col)
                    .map_err(|_| IoError::Unsupported("column exceeds XLSX grid".into()))?;
                out.write_number(corner.row, col, 0.)?;
            }
        }
        stored_formulas.push(rewritten_formulas);
    }
    let generated = writer.save_to_buffer()?;
    ooxml::patch_export(&generated, workbook, options, &styles, &stored_formulas)
}
/// Plan and serialize XLSX before writing the completed bytes to a file.
pub fn write_xlsx(workbook: &impl WorkbookSource, path: impl AsRef<Path>) -> Result<(), IoError> {
    fs::write(path, write_xlsx_to_buffer(workbook)?)?;
    Ok(())
}

/// Writes the destination only after serialization and all budget checks succeed.
pub fn write_xlsx_with_options(
    workbook: &impl WorkbookSource,
    path: impl AsRef<Path>,
    options: WriteOptions,
) -> Result<(), IoError> {
    fs::write(path, write_xlsx_to_buffer_with_options(workbook, options)?)?;
    Ok(())
}
