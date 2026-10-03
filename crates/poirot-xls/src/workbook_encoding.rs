//! Deterministic BIFF Workbook layout from the immutable export plan.
use crate::record_layout::{bof, long, record, short};
use crate::representation::{cached, error_code, unicode};
use crate::{XlsError, XlsExportPlan, context, fail, strings};
use poirot_model::{CellValue, DateSystem, TextRef};
/// Write an actual BIFF8 Workbook stream inside a version-3 CFB container.
pub(super) fn encode_biff(plan: &XlsExportPlan<'_>) -> Result<Vec<u8>, XlsError> {
    let workbook = plan.workbook;
    if workbook.sheets().is_empty() {
        return Err(fail("XLS output requires at least one worksheet"));
    }
    if workbook.sheets().len() > 1024 {
        return Err(fail("worksheet count exceeds export resource limit"));
    }
    let strings = &plan.strings;
    let formats = &plan.formats;
    let total_strings = plan.total_strings;
    let mut globals = Vec::new();
    record(&mut globals, 0x0809, &bof(5))?;
    record(&mut globals, 0x0042, &1200u16.to_le_bytes())?;
    record(
        &mut globals,
        0x0022,
        &u16::from(workbook.date_system == DateSystem::Excel1904).to_le_bytes(),
    )?;
    let mut window = Vec::new();
    for n in [0u16, 0, 0x3fcf, 0x2a4e, 0x0038, 0, 0, 1, 600] {
        short(&mut window, n)
    }
    record(&mut globals, 0x003d, &window)?;
    let mut font = Vec::new();
    for n in [200u16, 0, 0x7fff, 400, 0] {
        short(&mut font, n)
    }
    font.extend([0, 0, 0, 0, 5, 0]);
    font.extend(b"Arial");
    record(&mut globals, 0x0031, &font)?;
    let mut xf = vec![0u8; 20];
    // SAFETY: xf is created with twenty bytes and custom clones it; window is created with eighteen bytes, so these fixed field offsets fit.
    // SAFETY: The checked destination range and the byte array each contain 2 bytes.
    xf[4..6].copy_from_slice(&0xfff5u16.to_le_bytes());
    record(&mut globals, 0x00e0, &xf)?;
    // SAFETY: xf is created with twenty bytes and custom clones it; window is created with eighteen bytes, so these fixed field offsets fit.
    // SAFETY: The checked destination range and the byte array each contain 2 bytes.
    xf[4..6].copy_from_slice(&1u16.to_le_bytes());
    record(&mut globals, 0x00e0, &xf)?;
    let mut ordered_formats: Vec<_> = formats.iter().collect();
    ordered_formats.sort_by_key(|(_, i)| **i);
    for (format, i) in ordered_formats {
        let units = unicode(format)?;
        if units.len() > 4000 {
            return Err(fail("number format too long for BIFF FORMAT record"));
        }
        let mut d = Vec::new();
        short(&mut d, 164 + i);
        short(&mut d, units.len() as u16);
        d.push(1);
        for u in units {
            short(&mut d, u)
        }
        record(&mut globals, 0x041e, &d)?;
        let mut custom = xf.clone();
        // SAFETY: xf is created with twenty bytes and custom clones it; window is created with eighteen bytes, so these fixed field offsets fit.
        // SAFETY: The checked destination range and the byte array each contain 2 bytes.
        custom[2..4].copy_from_slice(&(164 + i).to_le_bytes());
        // SAFETY: xf is created with twenty bytes and custom clones it; window is created with eighteen bytes, so these fixed field offsets fit.
        custom[9] = 0x04;
        record(&mut globals, 0x00e0, &custom)?;
    }
    record(&mut globals, 0x0293, &[0, 0x80, 0, 0])?;
    let mut bound_positions = Vec::new();
    for sheet in workbook.sheets() {
        let name: Vec<_> = sheet.name().encode_utf16().collect();
        let mut d = Vec::new();
        long(&mut d, 0);
        short(&mut d, 0);
        d.extend([name.len() as u8, 1]);
        for u in name {
            short(&mut d, u)
        }
        bound_positions.push(globals.len() + 4);
        record(&mut globals, 0x0085, &d)?;
    }
    let mut sup = Vec::new();
    short(&mut sup, workbook.sheets().len() as u16);
    short(&mut sup, 0x0401);
    record(&mut globals, 0x01ae, &sup)?;
    plan.addins.encode_records(&mut globals)?;
    let mut ext = Vec::new();
    short(
        &mut ext,
        (workbook.sheets().len() + usize::from(!plan.addins.is_empty())) as u16,
    );
    for i in 0..workbook.sheets().len() {
        short(&mut ext, 0);
        short(&mut ext, i as u16);
        short(&mut ext, i as u16)
    }
    if !plan.addins.is_empty() {
        short(&mut ext, 1);
        short(&mut ext, 0xfffe);
        short(&mut ext, 0xfffe);
    }
    record(&mut globals, 0x0017, &ext)?;
    for (name_index, named) in workbook.named_ranges.iter().enumerate() {
        let sheet = named.sheet_scope.unwrap_or(0);
        if sheet >= workbook.sheets().len() {
            return Err(fail("defined name has invalid sheet scope"));
        }
        let tokens = &plan
            .names
            .get(name_index)
            .ok_or_else(|| fail("missing defined-name plan"))?
            .tokens;
        let name = unicode(&named.name)?;
        if name.len() > 255 {
            return Err(fail("defined name exceeds BIFF byte length"));
        }
        let mut d = Vec::new();
        short(&mut d, 0);
        d.extend([0, name.len() as u8]);
        short(&mut d, tokens.len() as u16);
        short(&mut d, 0);
        short(&mut d, named.sheet_scope.map_or(0, |i| i as u16 + 1));
        let wide = name.iter().any(|u| *u > 255);
        d.extend([0, 0, 0, 0, u8::from(wide)]);
        for u in name {
            if wide {
                short(&mut d, u)
            } else {
                d.push(u as u8)
            }
        }
        d.extend(tokens);
        record(&mut globals, 0x0018, &d)?;
    }
    let mut ordered = vec![TextRef::Utf8(""); strings.len()];
    for (s, i) in strings {
        *ordered
            .get_mut(*i as usize)
            .ok_or_else(|| fail("invalid shared-string index"))? = *s
    }
    let mut sst = strings::Continued::new(0x00fc);
    long(&mut sst.data, total_strings);
    long(&mut sst.data, strings.len() as u32);
    for s in &ordered {
        sst.string(*s)?
    }
    sst.finish(&mut globals)?;
    record(&mut globals, 0x000a, &[])?;
    let mut sheets = Vec::new();
    for (sheet_index, sheet) in workbook.sheets().iter().enumerate() {
        let mut out = Vec::new();
        record(&mut out, 0x0809, &bof(0x10))?;
        let mut dim = Vec::new();
        let min_row = sheet.row_indices().min().unwrap_or(0);
        let max_row = sheet.row_indices().map(|row| row + 1).max().unwrap_or(0);
        let min_col = sheet.cells().keys().map(|a| a.col).min().unwrap_or(0);
        let max_col = sheet.cells().keys().map(|a| a.col + 1).max().unwrap_or(0);
        long(&mut dim, min_row);
        long(&mut dim, max_row);
        short(&mut dim, min_col as u16);
        short(&mut dim, max_col as u16);
        short(&mut dim, 0);
        record(&mut out, 0x0200, &dim)?;
        for row in sheet.row_indices() {
            // BIFF8 ROW: zero based row, empty column bounds, default height,
            // reserved fields and default row options; cell records follow.
            let mut data = Vec::new();
            short(&mut data, row as u16);
            let view = sheet.row(row).ok_or_else(|| fail("missing source row"))?;
            short(
                &mut data,
                view.cells().next().map_or(0, |(col, _)| col as u16),
            );
            short(
                &mut data,
                view.cells()
                    .next_back()
                    .map_or(0, |(col, _)| col as u16 + 1),
            );
            short(&mut data, 0x00ff);
            short(&mut data, 0);
            short(&mut data, 0);
            long(&mut data, 0x000f0100);
            record(&mut out, 0x0208, &data)?;
        }
        for (a, c) in sheet.cells() {
            let result = (|| {
                let mut d = Vec::new();
                short(&mut d, a.row as u16);
                short(&mut d, a.col as u16);
                short(
                    &mut d,
                    c.style
                        .number_format
                        .as_ref()
                        .map_or(1, |f| formats[f.as_str()] + 2),
                );
                if let Some(formula) = &c.formula {
                    let _ = formula;
                    let tokens = &plan
                        .cells
                        .get(&(sheet_index, *a))
                        .ok_or_else(|| fail("missing formula export plan"))?
                        .tokens;
                    if tokens.len() > 8192 - 22 {
                        return Err(fail("formula token stream exceeds BIFF record limit"));
                    }
                    d.extend(cached(&c.value)?);
                    short(&mut d, 0);
                    long(&mut d, 0);
                    short(&mut d, tokens.len() as u16);
                    d.extend(tokens);
                    record(&mut out, 0x0006, &d)?;
                    if let Some(s) = c.value.text_ref() {
                        let mut value = strings::Continued::new(0x0207);
                        value.string(s)?;
                        value.finish(&mut out)?
                    }
                    return Ok(());
                }
                let id = match &c.value {
                    CellValue::Blank => 0x0201,
                    CellValue::Number(n) => {
                        if !n.is_finite() {
                            return Err(fail("nonfinite numeric cell"));
                        }
                        d.extend(n.to_le_bytes());
                        0x0203
                    }
                    CellValue::Text(_) | CellValue::TextUtf16(_) => {
                        long(
                            &mut d,
                            *strings
                                .get(
                                    &c.value
                                        .text_ref()
                                        .ok_or_else(|| fail("invalid text value"))?,
                                )
                                .ok_or_else(|| fail("missing shared-string value"))?,
                        );
                        0x00fd
                    }
                    CellValue::Boolean(b) => {
                        d.extend([u8::from(*b), 0]);
                        0x0205
                    }
                    CellValue::Error(e) => {
                        d.extend([error_code(*e)?, 1]);
                        0x0205
                    }
                };
                record(&mut out, id, &d)
            })();
            if let Err(e) = result {
                return Err(context(format!("{}!{}", sheet.name(), a), e));
            }
        }
        let mut window = vec![0u8; 18];
        // SAFETY: xf is created with twenty bytes and custom clones it; window is created with eighteen bytes, so these fixed field offsets fit.
        // SAFETY: The checked destination range and the byte array each contain 2 bytes.
        window[..2].copy_from_slice(&0x06b6u16.to_le_bytes());
        record(&mut out, 0x023e, &window)?;
        record(&mut out, 0x000a, &[])?;
        sheets.push(out)
    }
    let mut position = globals.len();
    for (patch, sheet) in bound_positions.into_iter().zip(sheets) {
        let offset = u32::try_from(position).map_err(|_| fail("Workbook stream exceeds 4GiB"))?;
        // SAFETY: The checked destination range and the byte array each contain 4 bytes.
        globals
            .get_mut(
                patch
                    ..patch
                        .checked_add(4)
                        .ok_or_else(|| fail("sheet offset overflow"))?,
            )
            .ok_or_else(|| fail("invalid sheet offset patch"))?
            .copy_from_slice(&offset.to_le_bytes());
        position += sheet.len();
        globals.extend(sheet)
    }
    Ok(globals)
}
