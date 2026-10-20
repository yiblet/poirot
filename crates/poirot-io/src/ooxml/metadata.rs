//! Ordered name, format, and physical-row import overlays.
use super::{
    package::Package,
    xml::{attr, events, part, text, xml},
};
use crate::IoError;
use poirot_model::{CellAddress, NamedRange, Workbook};
use poirot_model::{ImportDiagnostic, ImportDiagnosticKind};
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
    warnings: &mut Vec<ImportDiagnostic>,
) -> Result<(), IoError> {
    // Read workbook sheet states independently of the cell backend.
    let mut states = events(part(package, &plan.workbook_part)?);
    loop {
        match states.read_event().map_err(xml)? {
            Event::Start(e) | Event::Empty(e) if e.local_name().as_ref() == "sheet" => {
                let Some(name) = attr(&e, "name")? else {
                    continue;
                };
                let visibility = match attr(&e, "state")?.as_deref() {
                    None | Some("visible") => poirot_model::SheetVisibility::Visible,
                    Some("hidden") => poirot_model::SheetVisibility::Hidden,
                    Some("veryHidden") => poirot_model::SheetVisibility::VeryHidden,
                    Some(_) => return Err(IoError::Read("invalid worksheet visibility".into())),
                };
                if let Some(sheet) = workbook.sheet_by_name_mut(&name) {
                    let mut p = sheet.presentation();
                    p.visibility = visibility;
                    sheet.set_presentation(p);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let mut reader = events(part(package, &identity.part)?);
        let mut in_properties = false;
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == "sheetPr" => in_properties = true,
                Event::End(e) if e.local_name().as_ref() == "sheetPr" => in_properties = false,
                Event::Start(e) | Event::Empty(e)
                    if in_properties && e.local_name().as_ref() == "tabColor" =>
                {
                    let rgb = attr(&e, "rgb")?;
                    let other = ["theme", "indexed", "auto", "tint"]
                        .into_iter()
                        .map(|key| attr(&e, key))
                        .collect::<Result<Vec<_>, _>>()?
                        .iter()
                        .any(Option::is_some);
                    let channels = rgb
                        .as_deref()
                        .filter(|_| !other)
                        .filter(|v| v.len() == 8)
                        .and_then(|v| u32::from_str_radix(v, 16).ok())
                        .filter(|n| n >> 24 == 0xff)
                        .map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8]);
                    if let Some(color) = channels {
                        if let Some(sheet) = workbook.sheet_by_name_mut(&identity.name) {
                            let mut p = sheet.presentation();
                            p.tab_color = Some(color);
                            sheet.set_presentation(p);
                        }
                    } else {
                        warnings.push(ImportDiagnostic { kind: ImportDiagnosticKind::RepresentationLoss, detail: format!("{}: unsupported tab color representation omitted (only opaque RGB is retained)", identity.name) });
                    }
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    let mut reader = events(part(package, &plan.workbook_part)?);
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == "definedName" => {
                let name = attr(&e, "name")?
                    .ok_or_else(|| IoError::Read("defined name without name".into()))?;
                let scope = attr(&e, "localSheetId")?
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
                            warnings.push(ImportDiagnostic {
                                kind: ImportDiagnosticKind::RepresentationLoss,
                                detail: format!(
                                    "defined name {name} scoped to omitted sheet {} is omitted",
                                    identity.name
                                ),
                            });
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
            Event::Start(e) if e.local_name().as_ref() == "numFmt" => {
                if let (Some(id), Some(code)) = (attr(&e, "numFmtId")?, attr(&e, "formatCode")?) {
                    formats.insert(id.parse::<u32>().map_err(xml)?, code);
                }
            }
            Event::Start(e) if e.local_name().as_ref() == "cellXfs" => in_cell_xfs = true,
            Event::End(e) if e.local_name().as_ref() == "cellXfs" => in_cell_xfs = false,
            Event::Start(e) if in_cell_xfs && e.local_name().as_ref() == "xf" => xf.push(
                attr(&e, "numFmtId")?
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
                Event::Start(e) if e.local_name().as_ref() == "c" => {
                    let Some(reference) = attr(&e, "r")? else {
                        continue;
                    };
                    let address: CellAddress = reference.parse()?;
                    let style = attr(&e, "s")?
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
                            warnings.push(ImportDiagnostic {
                                kind: ImportDiagnosticKind::RepresentationLoss,
                                detail: format!(
                                    "number-format ID {id} at {}!{address} has no known raw code",
                                    sheet.name()
                                ),
                            });
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
                Event::Start(e) if e.local_name().as_ref() == "row" => {
                    let index = match attr(&e, "r")? {
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
