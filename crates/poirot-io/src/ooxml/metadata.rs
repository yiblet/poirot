//! Small source workbook and number-format metadata recovery.
use super::xml::{attr, events, text, xml};
use crate::IoError;
use poirot_model::{ImportDiagnostic, ImportDiagnosticKind, NamedRange, Workbook};
use quick_xml::events::Event;
pub(super) fn builtin(id: u32) -> Option<&'static str> {
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
/// Restore workbook names and sheet visibility without reading worksheet cells.
pub(crate) fn apply_workbook_metadata(
    bytes: &[u8],
    plan: &crate::opc::OpcReadPlan,
    workbook: &mut Workbook,
    warnings: &mut Vec<ImportDiagnostic>,
) -> Result<(), IoError> {
    // Read workbook sheet states independently of the cell backend.
    let mut states = events(bytes);
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
    let mut reader = events(bytes);
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == "definedName" => {
                let name = attr(&e, "name")?
                    .ok_or_else(|| IoError::Read("defined name without name".into()))?;
                let scope = attr(&e, "localSheetId")?
                    .map(|n| n.parse::<usize>().map_err(xml))
                    .transpose()?;
                let formula = text(&mut reader, e.name())?;
                if matches!(
                    name.as_str(),
                    "_xlnm.Print_Area" | "_xlnm.Print_Titles" | "_xlnm._FilterDatabase"
                ) {
                    continue;
                }
                let scope = match scope {
                    Some(source_index) => {
                        let Some(identity) = plan.sheets.get(source_index) else {
                            warnings.push(ImportDiagnostic {
                                kind: ImportDiagnosticKind::RepresentationLoss,
                                detail: format!(
                                    "defined name {name}: invalid local sheet scope omitted"
                                ),
                            });
                            continue;
                        };
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
    Ok(())
}
