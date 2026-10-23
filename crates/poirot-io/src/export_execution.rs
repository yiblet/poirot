//! XLSX serializer execution and write-after-success filesystem egress.
use crate::{IoError, WriteOptions, ooxml, plan_xlsx_export};
use poirot_model::{CellValue, Workbook};
use rust_xlsxwriter::{Format, Formula};
use std::{fs, path::Path};

/// Writes XLSX with explicit writer budgets; no recalculation is performed.
pub fn write_xlsx_to_buffer_with_options(
    workbook: &Workbook,
    options: WriteOptions,
) -> Result<Vec<u8>, IoError> {
    plan_xlsx_export(workbook, options)?.execute()
}
/// Uses the documented [`WriteOptions::default`] compatibility policy.
pub fn write_xlsx_to_buffer(workbook: &Workbook) -> Result<Vec<u8>, IoError> {
    write_xlsx_to_buffer_with_options(workbook, WriteOptions::default())
}
/// Encode an immutable source workbook into an admitted in-memory XLSX package.
pub(super) fn serialize_xlsx(
    workbook: &Workbook,
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
        .position(|s| s.presentation().visibility == poirot_model::SheetVisibility::Visible)
        .ok_or_else(|| {
            IoError::Unsupported("cannot export workbook without a visible worksheet".into())
        })?;
    let mut writer = rust_xlsxwriter::Workbook::new();
    for (index, sheet) in workbook.sheets().iter().enumerate() {
        let out = writer.add_worksheet();
        out.set_name(sheet.name())?;
        if index == active {
            out.set_active(true);
            out.set_first_tab(true);
        }
        match sheet.presentation().visibility {
            poirot_model::SheetVisibility::Visible => {}
            poirot_model::SheetVisibility::Hidden => {
                out.set_hidden(true);
            }
            poirot_model::SheetVisibility::VeryHidden => {
                out.set_very_hidden(true);
            }
        }
        if let Some([r, g, b]) = sheet.presentation().tab_color {
            out.set_tab_color(rust_xlsxwriter::Color::RGB(
                (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
            ));
        }
        let spills = ooxml::spill_bounds(sheet)?;
        for (address, cell) in sheet.cells() {
            address.validate()?;
            crate::numeric_representation::check(sheet.name(), *address, cell)?;
            let col = address.col as u16;
            let format = cell
                .style
                .number_format
                .as_ref()
                .map_or_else(Format::new, |f| Format::new().set_num_format(f));
            if let Some(formula) = &cell.formula {
                let formula = crate::ooxml::formula::stored_formula(formula)?;
                if let Some(last) = spills.get(address) {
                    out.write_dynamic_array_formula_with_format(
                        address.row,
                        col,
                        last.row,
                        last.col as u16,
                        Formula::new(formula).set_result("0"),
                        &format,
                    )?;
                } else {
                    out.write_formula_with_format(
                        address.row,
                        col,
                        Formula::new(formula).set_result("0"),
                        &format,
                    )?;
                }
            } else {
                match &cell.value {
                    CellValue::Number(n) => {
                        if !n.is_finite() {
                            return Err(IoError::Unsupported(format!(
                                "nonfinite value at {}!{address}",
                                sheet.name()
                            )));
                        }
                        out.write_number_with_format(address.row, col, *n, &format)?;
                    }
                    CellValue::Text(_) | CellValue::TextUtf16(_) => {
                        // The lossless XML patch supplies authoritative text.
                        out.write_blank(address.row, col, &format)?;
                    }
                    CellValue::Boolean(b) => {
                        out.write_boolean_with_format(address.row, col, *b, &format)?;
                    }
                    CellValue::Blank | CellValue::Error(_) => {
                        out.write_blank(address.row, col, &format)?;
                    }
                }
            }
        }
    }
    let generated = writer.save_to_buffer()?;
    ooxml::patch_export(&generated, workbook, options)
}
/// Plan and serialize XLSX before writing the completed bytes to a file.
pub fn write_xlsx(workbook: &Workbook, path: impl AsRef<Path>) -> Result<(), IoError> {
    fs::write(path, write_xlsx_to_buffer(workbook)?)?;
    Ok(())
}

/// Writes the destination only after serialization and all budget checks succeed.
pub fn write_xlsx_with_options(
    workbook: &Workbook,
    path: impl AsRef<Path>,
    options: WriteOptions,
) -> Result<(), IoError> {
    fs::write(path, write_xlsx_to_buffer_with_options(workbook, options)?)?;
    Ok(())
}
