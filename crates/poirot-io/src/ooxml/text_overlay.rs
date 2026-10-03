//! Authoritative text overlay from the checked original package, never the
//! backend's scalar-only string view. Rich-text runs concatenate in order.
use super::{
    package::Package,
    text::decode,
    xml::{attr, events, part, text, xml},
};
use crate::{IoError, opc::OpcReadPlan};
use poirot_model::{Cell, CellAddress, CellValue, ExcelText, Workbook};
use quick_xml::{Reader, events::Event};

fn rich(reader: &mut Reader<&[u8]>, end: &[u8]) -> Result<ExcelText, IoError> {
    let mut units = Vec::new();
    loop {
        match reader.read_event().map_err(xml)? {
            Event::Start(e) if e.local_name().as_ref() == b"rPh" => {
                reader.read_to_end(e.name()).map_err(xml)?;
            }
            Event::Start(e) if e.local_name().as_ref() == b"t" => {
                units.extend_from_slice(decode(&text(reader, e.name())?).units());
            }
            Event::End(e) if e.local_name().as_ref() == end => break,
            Event::Eof => return Err(IoError::Xml("truncated SpreadsheetML text".into())),
            _ => {}
        }
    }
    Ok(ExcelText::from_units(units))
}

/// Restore exact text units from checked package parts into decoded cells.
pub(crate) fn apply_text(
    package: &Package,
    plan: &OpcReadPlan,
    workbook: &mut Workbook,
) -> Result<(), IoError> {
    let mut shared = Vec::new();
    if let Some(path) = &plan.shared_strings_part {
        let mut reader = events(part(package, path)?);
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == b"si" => {
                    shared.push(rich(&mut reader, b"si")?)
                }
                Event::Eof => break,
                _ => {}
            }
        }
    }
    for identity in plan.sheets.iter().filter(|s| s.supported) {
        let sheet = workbook
            .sheet_by_name_mut(&identity.name)
            .ok_or_else(|| IoError::Read("missing text worksheet".into()))?;
        let mut reader = events(part(package, &identity.part)?);
        let mut next_row = 0;
        let mut row = 0;
        let mut next_col = 0;
        loop {
            match reader.read_event().map_err(xml)? {
                Event::Start(e) if e.local_name().as_ref() == b"row" => {
                    row = match attr(&reader, &e, b"r")? {
                        Some(r) => r
                            .parse::<u32>()
                            .map_err(xml)?
                            .checked_sub(1)
                            .ok_or_else(|| IoError::Read("zero row index".into()))?,
                        None => next_row,
                    };
                    next_row = row + 1;
                    next_col = 0;
                }
                Event::Start(e) if e.local_name().as_ref() == b"c" => {
                    let address = match attr(&reader, &e, b"r")? {
                        Some(r) => r.parse::<CellAddress>()?,
                        None => CellAddress::new(row, next_col)?,
                    };
                    next_col = address.col + 1;
                    let kind = attr(&reader, &e, b"t")?;
                    if !matches!(kind.as_deref(), Some("s" | "str" | "inlineStr")) {
                        continue;
                    }
                    let mut value = None;
                    loop {
                        match reader.read_event().map_err(xml)? {
                            Event::Start(e) if e.local_name().as_ref() == b"is" => {
                                value = Some(rich(&mut reader, b"is")?)
                            }
                            Event::Start(e) if e.local_name().as_ref() == b"v" => {
                                let raw = text(&mut reader, e.name())?;
                                value = Some(if kind.as_deref() == Some("s") {
                                    let index = raw.parse::<usize>().map_err(xml)?;
                                    shared.get(index).cloned().ok_or_else(|| {
                                        IoError::Read("shared string index out of range".into())
                                    })?
                                } else {
                                    decode(&raw)
                                });
                            }
                            Event::End(e) if e.local_name().as_ref() == b"c" => break,
                            Event::Eof => return Err(IoError::Xml("truncated text cell".into())),
                            _ => {}
                        }
                    }
                    // An absent cache remains blank; an explicit empty string is text.
                    if let Some(value) = value {
                        if let Some(cell) = sheet.get_mut(address) {
                            cell.value = CellValue::TextUtf16(value);
                        } else {
                            sheet.set(address, Cell::new(CellValue::TextUtf16(value)))?;
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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(
            workbook
                .sheet(0)
                .unwrap()
                .get(CellAddress::new(0, 5).unwrap())
                .is_none()
        );
        assert_eq!(package, original);
    }
}
