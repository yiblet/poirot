//! Ordered name, format, and physical-row import overlays.
use super::{
    package::Package,
    xml::{attr, events, part, text, xml},
};
use crate::IoError;
use poirot_model::{CellAddress, NamedRange, Workbook};
use quick_xml::events::Event;
use std::collections::BTreeMap;

fn builtin(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}
/// Restore checked workbook metadata omitted or altered by backend decoding.
pub(crate) fn apply_metadata(
    package: &Package,
    plan: &crate::opc::OpcReadPlan,
    workbook: &mut Workbook,
    warnings: &mut Vec<String>,
) -> Result<(), IoError> {
    let mut reader = events(part(package, &plan.workbook_part)?);
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == b"definedName" => {
                let name = attr(&reader, &e, b"name")?
                    .ok_or_else(|| IoError::Read("defined name without name".into()))?;
                let scope = attr(&reader, &e, b"localSheetId")?
                    .map(|n| n.parse::<usize>().map_err(xml))
                    .transpose()?;
                let formula = text(&mut reader, e.name())?;
                let scope = match scope {
                    Some(source_index) => {
                        let identity = plan.sheets.get(source_index).ok_or_else(|| {
                            IoError::Opc(crate::opc::OpcError::SheetMismatch(format!(
                                "defined name {name}: localSheetId out of range"
                            )))
                        })?;
                        if !identity.supported {
                            warnings.push(format!(
                                "defined name {name} scoped to omitted sheet {} is omitted",
                                identity.name
                            ));
                            continue;
                        }
                        Some(
                            workbook
                                .sheets()
                                .iter()
                                .position(|sheet| sheet.name() == identity.name)
                                .ok_or_else(|| {
                                    IoError::Opc(crate::opc::OpcError::SheetMismatch(
                                        identity.name.clone(),
                                    ))
                                })?,
                        )
                    }
                    None => None,
                };
                workbook.named_ranges.push(NamedRange {
                    name,
                    formula: formula.trim_start_matches('=').into(),
                    sheet_scope: scope,
                });
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let Some(styles) = plan.styles_part.as_ref().and_then(|path| package.get(path)) else {
        return Ok(());
    };
    let mut formats = BTreeMap::new();
    let mut xf = Vec::new();
    let mut in_cell_xfs = false;
    let mut reader = events(styles);
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == b"numFmt" => {
                if let (Some(id), Some(code)) = (
                    attr(&reader, &e, b"numFmtId")?,
                    attr(&reader, &e, b"formatCode")?,
                ) {
                    formats.insert(id.parse::<u32>().map_err(xml)?, code);
                }
            }
            Event::Start(e) if e.local_name().as_ref() == b"cellXfs" => in_cell_xfs = true,
            Event::End(e) if e.local_name().as_ref() == b"cellXfs" => in_cell_xfs = false,
            Event::Start(e) if in_cell_xfs && e.local_name().as_ref() == b"xf" => xf.push(
                attr(&reader, &e, b"numFmtId")?
                    .unwrap_or_else(|| "0".into())
                    .parse::<u32>()
                    .map_err(xml)?,
            ),
            Event::Eof => break,
            _ => {}
        }
    }
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let path = &identity.part;
        let index = workbook
            .sheets()
            .iter()
            .position(|s| s.name() == identity.name)
            .ok_or_else(|| {
                IoError::Opc(crate::opc::OpcError::SheetMismatch(identity.name.clone()))
            })?;
        let sheet = workbook.sheet_mut(index).ok_or_else(|| {
            IoError::Opc(crate::opc::OpcError::SheetMismatch(identity.name.clone()))
        })?;
        let mut reader = events(part(package, path)?);
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == b"c" => {
                    let Some(reference) = attr(&reader, &e, b"r")? else {
                        continue;
                    };
                    let address: CellAddress = reference.parse()?;
                    let style = attr(&reader, &e, b"s")?
                        .unwrap_or_else(|| "0".into())
                        .parse::<usize>()
                        .map_err(xml)?;
                    if let Some(&id) = xf.get(style) {
                        if id == 0 {
                            continue;
                        }
                        let code = formats
                            .get(&id)
                            .cloned()
                            .or_else(|| builtin(id).map(str::to_owned));
                        if let Some(code) = code {
                            if sheet.get(address).is_none() {
                                sheet.set(address, poirot_model::Cell::default())?;
                            }
                            if let Some(cell) = sheet.get_mut(address) {
                                cell.style.number_format = Some(code);
                            }
                        } else {
                            warnings.push(format!(
                                "number-format ID {id} at {}!{address} has no known raw code",
                                sheet.name()
                            ));
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(())
}
/// Preserve physical row records separately from style metadata (which may be absent).
pub(crate) fn apply_rows(
    package: &BTreeMap<String, Vec<u8>>,
    plan: &crate::opc::OpcReadPlan,
    workbook: &mut Workbook,
) -> Result<(), IoError> {
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let sheet = workbook
            .sheet_by_name_mut(&identity.name)
            .ok_or_else(|| IoError::Read("missing row worksheet".into()))?;
        let mut reader = events(part(package, &identity.part)?);
        let mut next_row = 0u32;
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == b"row" => {
                    let index = match attr(&reader, &e, b"r")? {
                        Some(r) => r
                            .parse::<u32>()
                            .map_err(xml)?
                            .checked_sub(1)
                            .ok_or_else(|| IoError::Read("zero row index".into()))?,
                        None => next_row,
                    };
                    sheet.create_row(index)?;
                    next_row = index + 1;
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(())
}
