//! XLSX serializer execution and write-after-success filesystem egress.
use crate::CellValue;
use crate::data::{SheetSource, WorkbookSource};
use crate::{IoError, WriteOptions, ooxml, plan_xlsx_export};
use rxls::Format;
use std::{fs, path::Path};

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
    let mut writer = rxls::Workbook::new();
    writer.set_active_sheet(active);
    writer.date1904 = workbook.date_system() == crate::DateSystem::Excel1904;
    for sheet in workbook.sheets() {
        let out = writer.add_sheet(sheet.name());
        match sheet.presentation().visibility {
            crate::SheetVisibility::Visible => {}
            crate::SheetVisibility::Hidden => out.hide(),
            crate::SheetVisibility::VeryHidden => out.hide_very(),
        }
        if let Some([r, g, b]) = sheet.presentation().tab_color {
            out.set_tab_color(rxls::Color::rgb(r, g, b));
        }
        for (address, cell) in sheet.cells() {
            address.validate()?;
            crate::numeric_representation::check(sheet.name(), *address, cell)?;
            let col = u16::try_from(address.col)
                .map_err(|_| IoError::Unsupported("column exceeds XLSX grid".into()))?;
            styles.register(&cell.style)?;
            let format = Format::new();
            // Canonical XML projection below restores exact text, caches, spills,
            // presentation and errors; author only scalar placeholders here.
            if let Some(formula) = &cell.formula {
                let stored = crate::ooxml::formula::stored_formula(formula)?;
                out.write_formula_with_format(address.row, col, stored.as_str(), 0., &format);
            } else {
                match &cell.value {
                    CellValue::Number(n) => {
                        out.write_number_with_format(address.row, col, *n, &format)
                    }
                    CellValue::Boolean(b) => {
                        out.write_boolean_with_format(address.row, col, *b, &format)
                    }
                    _ => out.write_number_with_format(address.row, col, 0., &format),
                }
            }
        }
    }
    let generated = writer.to_xlsx_checked()?;
    ooxml::patch_export(&generated, workbook, options, &styles)
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
