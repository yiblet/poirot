//! Authoritative text overlay from the checked original package, never the
//! backend's scalar-only string view. Rich-text runs concatenate in order.
use super::{
    text::decode,
    xml::{attr, events, text, xml},
};
use crate::IoError;
use poirot_model::{Cell, CellAddress, CellValue, ExcelText};
use quick_xml::{Reader, events::Event};

fn rich(reader: &mut Reader<&[u8]>, end: &str) -> Result<ExcelText, IoError> {
    let mut units = Vec::new();
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == "rPh" => {
                reader.read_to_end(e.name()).map_err(xml)?;
            }
            Event::Start(e) if e.local_name().as_ref() == "t" => {
                units.extend_from_slice(decode(&text(reader, e.name())?).units());
            }
            Event::End(e) if e.local_name().as_ref() == end => break,
            Event::Eof => return Err(IoError::Xml("truncated SpreadsheetML text".into())),
            _ => {}
        }
    }
    Ok(ExcelText::from_units(units))
}

/// Decode original shared strings once for per-sheet text recovery.
pub(crate) fn shared_strings(bytes: Option<&[u8]>) -> Result<Vec<ExcelText>, IoError> {
    let mut shared = Vec::new();
    if let Some(bytes) = bytes {
        let mut reader = events(bytes);
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == "si" => {
                    shared.push(rich(&mut reader, "si")?)
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    Ok(shared)
}

/// Restore one sheet's calculation-visible source data and detect spill recovery.
pub(crate) fn apply_sheet_text(
    bytes: &[u8],
    shared: &[ExcelText],
    sheet: &mut poirot_model::Worksheet,
    formats: &[Option<String>],
) -> Result<bool, IoError> {
    let mut needs_spills = false;
    let mut reader = events(bytes);
    let mut next_row = 0;
    let mut row = 0;
    let mut next_col = 0;
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == "tabColor" => {
                if let Some(rgb) = attr(&e, "rgb")?
                    .filter(|v| v.len() == 8)
                    .and_then(|v| u32::from_str_radix(&v, 16).ok())
                    .filter(|n| n >> 24 == 0xff)
                {
                    let mut presentation = sheet.presentation();
                    presentation.tab_color = Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
                    sheet.set_presentation(presentation);
                }
            }
            Event::Start(e) if e.local_name().as_ref() == "row" => {
                row = match attr(&e, "r")? {
                    Some(r) => r
                        .parse::<u32>()
                        .map_err(xml)?
                        .checked_sub(1)
                        .ok_or_else(|| IoError::Read("zero row index".into()))?,
                    None => next_row,
                };
                sheet.create_row(row)?;
                if matches!(attr(&e, "hidden")?.as_deref(), Some("1" | "true")) {
                    sheet.set_row_visibility(
                        row,
                        poirot_model::RowVisibility {
                            manually_hidden: true,
                            filtered_out: false,
                        },
                    )?;
                }
                next_row = row + 1;
                next_col = 0;
            }
            Event::Start(e) if e.local_name().as_ref() == "c" => {
                let address = match attr(&e, "r")? {
                    Some(r) => r.parse::<CellAddress>()?,
                    None => CellAddress::new(row, next_col)?,
                };
                next_col = address.col + 1;
                let style = attr(&e, "s")?
                    .map(|s| s.parse::<usize>().map_err(xml))
                    .transpose()?
                    .unwrap_or(0);
                let kind = attr(&e, "t")?;
                if kind.as_deref() == Some("d") {
                    return Err(IoError::Unsupported(
                        "ISO date has no exact typed model representation".into(),
                    ));
                }
                let is_text = matches!(kind.as_deref(), Some("s" | "str" | "inlineStr"));
                let mut value = None;
                let mut has_value = false;
                let mut formula = None;
                let mut shared_formula = false;
                loop {
                    match reader.read_event().map_err(xml)? {
                        Event::Start(e) if e.local_name().as_ref() == "f" => {
                            let kind = attr(&e, "t")?;
                            shared_formula = kind.as_deref() == Some("shared");
                            needs_spills |= kind.as_deref() == Some("array");
                            let source = text(&mut reader, e.name())?;
                            if !shared_formula {
                                formula = Cell::with_formula(source, CellValue::Blank)?.formula;
                            }
                        }
                        Event::Start(e) if e.local_name().as_ref() == "is" => {
                            has_value = true;
                            value = Some(rich(&mut reader, "is")?)
                        }
                        Event::Start(e) if e.local_name().as_ref() == "v" => {
                            let raw = text(&mut reader, e.name())?;
                            has_value = true;
                            needs_spills |= kind.as_deref() == Some("e")
                                && matches!(raw.as_str(), "#SPILL!" | "#CALC!");
                            if !is_text {
                                continue;
                            }
                            value = Some(if kind.as_deref() == Some("s") {
                                let index = raw.parse::<usize>().map_err(xml)?;
                                shared.get(index).cloned().ok_or_else(|| {
                                    IoError::Read("shared string index out of range".into())
                                })?
                            } else {
                                decode(&raw)
                            });
                        }
                        Event::End(e) if e.local_name().as_ref() == "c" => break,
                        Event::Eof => return Err(IoError::Xml("truncated text cell".into())),
                        _ => {}
                    }
                }
                if let Some(cell) = sheet.get_mut(address) {
                    if !shared_formula {
                        cell.formula = formula;
                    }
                    if let Some(value) = value {
                        cell.value = CellValue::TextUtf16(value);
                    } else if !has_value {
                        cell.value = CellValue::Blank;
                    }
                } else if let Some(value) = value {
                    sheet.set(address, Cell::new(CellValue::TextUtf16(value)))?;
                } else if !has_value {
                    let mut cell = Cell::new(CellValue::Blank);
                    cell.formula = formula;
                    sheet.set(address, cell)?;
                } else {
                    return Err(IoError::Read("backend omitted a stored cell value".into()));
                }
                if let Some(cell) = sheet.get_mut(address) {
                    cell.style.number_format = formats.get(style).cloned().flatten();
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(needs_spills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ooxml::{package::Package, xml::part},
        opc::OpcReadPlan,
    };
    use poirot_model::Workbook;

    /// Apply original package text for this module's recovery fixture.
    fn apply_text(
        package: &Package,
        plan: &OpcReadPlan,
        workbook: &mut Workbook,
    ) -> Result<(), IoError> {
        let bytes = plan
            .shared_strings_part
            .as_ref()
            .map(|path| part(package, path))
            .transpose()?;
        let shared = shared_strings(bytes)?;
        for identity in plan.sheets.iter().filter(|s| s.supported) {
            let sheet = workbook
                .sheet_by_name_mut(&identity.name)
                .ok_or_else(|| IoError::Read("missing text worksheet".into()))?;
            apply_sheet_text(part(package, &identity.part)?, &shared, sheet, &[])?;
        }
        Ok(())
    }

    #[test]
    fn original_xml_shared_inline_rich_and_true_formula_cache_units() {
        let mut package = Package::new();
        package.insert("shared.xml".into(), br#"<sst><si><r><t>_xD83D_</t></r><rPh><t>ignored</t></rPh><r><t>_xDE00_</t></r></si><si><t>_x005F_xD83D_</t></si></sst>"#.to_vec());
        package.insert("sheet.xml".into(), br#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="inlineStr"><is><t>_xD83D_</t></is></c><c r="C1" t="str"><f>RIGHT(A1,1)</f><v>_xDE00_</v></c><c r="D1" t="s"><v>1</v></c><c r="E1" t="str"><v></v></c><c r="F1" t="str"><f>1</f></c></row></sheetData></worksheet>"#.to_vec());
        let original = package.clone();
        let plan = OpcReadPlan {
            workbook_part: "book.xml".into(),
            styles_part: None,
            shared_strings_part: Some("shared.xml".into()),
            warnings: vec![],
            sheets: vec![crate::opc::SheetIdentity {
                source_index: 0,
                name: "Data".into(),
                sheet_id: "1".into(),
                relationship_id: "rId1".into(),
                part: "sheet.xml".into(),
                supported: true,
            }],
        };
        let mut workbook = Workbook::new();
        workbook.add_sheet("Data").unwrap();
        apply_text(&package, &plan, &mut workbook).unwrap();
        for (col, expected) in [
            (0, vec![0xd83d, 0xde00]),
            (1, vec![0xd83d]),
            (2, vec![0xde00]),
            (3, "_xD83D_".encode_utf16().collect()),
            (4, vec![]),
        ] {
            let cell = workbook
                .sheet(0)
                .unwrap()
                .get(CellAddress::new(0, col).unwrap())
                .unwrap();
            assert_eq!(cell.value, CellValue::from(ExcelText::from_units(expected)));
        }
        let missing_cache = workbook
            .sheet(0)
            .unwrap()
            .get(CellAddress::new(0, 5).unwrap())
            .unwrap();
        assert_eq!(missing_cache.value, CellValue::Blank);
        assert_eq!(missing_cache.formula.as_deref(), Some("1"));
        assert_eq!(package, original);
    }
}
